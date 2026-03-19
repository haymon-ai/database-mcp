#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# run.sh — Run functional tests against database containers
#
# Each test uses #[sqlx::test] for per-test database isolation. Docker
# containers only need to provide a running server — schema and seed data
# are applied per test via sqlx migrations.
#
# Readiness is determined by Docker Compose healthchecks defined in
# compose.yml — no custom polling logic needed.
#
# Usage:
#   ./tests/run.sh                     # Run full matrix
#   ./tests/run.sh --filter mariadb    # All MariaDB versions
#   ./tests/run.sh --filter mysql_9    # Specific service
#   ./tests/run.sh --help              # Show usage
#
# Environment:
#   TIMEOUT=60   Container healthcheck timeout in seconds (default: 60)
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/compose.yml"
TIMEOUT="${TIMEOUT:-60}"

# Matrix: service_name:db_type:container_port:test_binary
#   service_name   — matches compose.yml service
#   db_type        — used for DATABASE_URL construction
#   container_port — internal port to resolve via `docker compose port`
#   test_binary    — cargo test binary name (from [[test]] in Cargo.toml)
MATRIX=(
    "mariadb_12:mysql:3306:mysql"
    "mysql_9:mysql:3306:mysql"
    "postgres_18:postgres:5432:postgres"
    "sqlite:sqlite:0:sqlite"
)

declare -a RESULTS=()
OVERALL_EXIT=0

cleanup() {
    echo ""
    echo "Cleaning up containers..."
    docker compose -f "$COMPOSE_FILE" down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

usage() {
    cat <<'EOF'
Usage: ./tests/run.sh [OPTIONS]

Run functional tests against database containers.

Options:
  --filter <pattern>   Run only services matching pattern (substring match)
  --help               Show this help message

Examples:
  ./tests/run.sh                   # Full matrix
  ./tests/run.sh --filter mariadb  # All MariaDB services
  ./tests/run.sh --filter postgres # All PostgreSQL services
  ./tests/run.sh --filter sqlite   # SQLite only

Environment:
  TIMEOUT=60   Container healthcheck timeout in seconds (default: 60)
EOF
}

check_docker() {
    if ! command -v docker &>/dev/null; then
        echo "ERROR: Docker is not installed or not in PATH."
        echo "Install Docker: https://docs.docker.com/get-docker/"
        exit 2
    fi
    if ! docker info &>/dev/null; then
        echo "ERROR: Docker daemon is not running."
        echo "Start Docker and try again."
        exit 2
    fi
}

wait_for_healthy() {
    local service="$1"
    local host_port="$2"
    local elapsed=0

    echo -n "  Waiting for healthy..."
    while [ $elapsed -lt "$TIMEOUT" ]; do
        local health
        health=$(docker compose -f "$COMPOSE_FILE" ps --format '{{.Health}}' "$service" 2>/dev/null || echo "")
        if [ "$health" = "healthy" ]; then
            # Verify host port is accepting connections (port mapping lag)
            if (echo > /dev/tcp/127.0.0.1/"$host_port") 2>/dev/null; then
                echo " OK (${elapsed}s)"
                return 0
            fi
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done

    echo " TIMEOUT after ${TIMEOUT}s"
    return 1
}

# ---------------------------------------------------------------------------
# Run one matrix entry
# ---------------------------------------------------------------------------

run_entry() {
    local service="$1"
    local db_type="$2"
    local container_port="$3"
    local test_bin="$4"

    echo ""
    echo "=== Testing ${service} ==="
    local start_time
    start_time=$(date +%s)

    local test_exit=0
    local test_output

    if [ "$db_type" = "sqlite" ]; then
        # SQLite: no container — sqlx::test auto-creates file-based databases
        echo "  Running cargo test... (sqlx::test manages databases)"
        test_output=$(
            cargo test --test "$test_bin" 2>&1
        ) || test_exit=$?
    else
        # Container-based databases (MySQL, MariaDB, PostgreSQL)
        echo -n "  Starting container..."
        if ! docker compose -f "$COMPOSE_FILE" up -d "$service" 2>/dev/null; then
            echo " FAILED"
            RESULTS+=("${service}|SKIP|0|$(( $(date +%s) - start_time ))")
            OVERALL_EXIT=1; return
        fi
        echo " OK"

        local host_port
        host_port=$(docker compose -f "$COMPOSE_FILE" port "$service" "$container_port" 2>/dev/null | cut -d: -f2)

        if ! wait_for_healthy "$service" "$host_port"; then
            echo "  Container failed to become healthy. Logs:"
            docker compose -f "$COMPOSE_FILE" logs "$service" 2>/dev/null | tail -20
            docker compose -f "$COMPOSE_FILE" stop "$service" 2>/dev/null || true
            docker compose -f "$COMPOSE_FILE" rm -f -v "$service" 2>/dev/null || true
            RESULTS+=("${service}|SKIP|0|$(( $(date +%s) - start_time ))")
            OVERALL_EXIT=1; return
        fi

        # Build DATABASE_URL for sqlx::test
        local database_url
        case "$db_type" in
            mysql)
                database_url="mysql://root:@127.0.0.1:${host_port}/mysql"
                ;;
            postgres)
                database_url="postgresql://postgres@127.0.0.1:${host_port}/postgres"
                ;;
        esac

        echo "  Running cargo test..."
        test_output=$(
            DATABASE_URL="$database_url" \
            cargo test --test "$test_bin" 2>&1
        ) || test_exit=$?

        echo -n "  Stopping container..."
        docker compose -f "$COMPOSE_FILE" stop "$service" 2>/dev/null || true
        docker compose -f "$COMPOSE_FILE" rm -f -v "$service" 2>/dev/null || true
        echo " OK"
    fi

    echo "$test_output" | grep -E "^(test |test result:)" || true

    local test_count
    test_count=$(echo "$test_output" | grep -oP '\d+ passed' | grep -oP '\d+' || echo "0")

    local duration=$(( $(date +%s) - start_time ))
    if [ "$test_exit" -eq 0 ]; then
        RESULTS+=("${service}|PASS|${test_count}|${duration}")
    else
        RESULTS+=("${service}|FAIL|${test_count}|${duration}")
        OVERALL_EXIT=1
        echo "  FAILED — see output above"
    fi
}

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

print_summary() {
    echo ""
    echo "╔══════════════════╦════════╦═══════╦══════════╗"
    echo "║ Service          ║ Status ║ Tests ║ Duration ║"
    echo "╠══════════════════╬════════╬═══════╬══════════╣"

    local total_tests=0 total_duration=0 fail_count=0 entry_count=0

    for result in "${RESULTS[@]}"; do
        IFS='|' read -r svc status tests duration <<< "$result"
        printf "║ %-16s ║ %-6s ║ %-5s ║ %6ss ║\n" "$svc" "$status" "$tests" "$duration"
        total_tests=$((total_tests + tests))
        total_duration=$((total_duration + duration))
        entry_count=$((entry_count + 1))
        [ "$status" != "PASS" ] && fail_count=$((fail_count + 1))
    done

    echo "╠══════════════════╬════════╬═══════╬══════════╣"

    local overall="PASS"
    [ "$fail_count" -gt 0 ] && overall="${fail_count} FAIL"
    local dfmt="${total_duration}s"
    [ "$total_duration" -ge 60 ] && dfmt="$((total_duration / 60))m $((total_duration % 60))s"

    printf "║ %-16s ║ %-6s ║ %-5s ║ %6s  ║\n" "TOTAL ($entry_count)" "$overall" "$total_tests" "$dfmt"
    echo "╚══════════════════╩════════╩═══════╩══════════╝"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

FILTER=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --filter) FILTER="$2"; shift 2 ;;
        --help|-h) usage; exit 0 ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

check_docker

echo "Database Functional Test Suite"
echo "=============================="
echo "Building project..."
cargo test --no-run 2>/dev/null || { echo "ERROR: Failed to build test binaries"; exit 2; }

for entry in "${MATRIX[@]}"; do
    IFS=':' read -r service db_type container_port test_bin <<< "$entry"
    [ -n "$FILTER" ] && [[ "$service" != *"$FILTER"* ]] && continue
    run_entry "$service" "$db_type" "$container_port" "$test_bin"
done

if [ ${#RESULTS[@]} -eq 0 ]; then
    echo "No matrix entries matched filter: $FILTER"
    exit 1
fi

print_summary
exit $OVERALL_EXIT
