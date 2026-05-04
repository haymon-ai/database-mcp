//! [`RowExt`](crate::RowExt) implementation for `PostgreSQL` rows.
//!
//! Type names are normalized to uppercase because sqlx may return either case
//! depending on the query context. Integer types use size-specific Rust types
//! (`i16`, `i32`, `i64`) because sqlx enforces strict type matching for
//! `PostgreSQL`. Temporal types (`DATE`, `TIME`, `TIMESTAMP`, `TIMESTAMPTZ`)
//! are decoded via sqlx's `chrono` integration and serialized as RFC 3339
//! strings; `TIMESTAMPTZ` is normalized to UTC and emitted with a `Z` suffix.
//! `NUMERIC` is decoded via `BigDecimal` to preserve precision; `MONEY`
//! arrives as text (sqlx uses simple-query for parameterless statements)
//! and is parsed locale-aware then routed through the same shape rule.

use std::str::FromStr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use bigdecimal::BigDecimal;
use serde_json::{Map, Value};
use sqlx::postgres::PgRow;
use sqlx::types::chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use sqlx::{Column, Row, TypeInfo, ValueRef};

use crate::RowExt;
use crate::numeric::bigdecimal_to_json;

/// Parses a locale-formatted Postgres `MONEY` text value into a `BigDecimal`.
///
/// Strips currency symbol and grouping separators (everything except digits,
/// `.`, and `-`), then parses what remains. Tuned for the en_US.UTF-8
/// `lc_monetary` default — locales using `,` as decimal separator are not
/// supported.
fn parse_pg_money_text(text: &str) -> Option<BigDecimal> {
    let cleaned: String = text.chars().filter(|c| matches!(c, '0'..='9' | '.' | '-')).collect();
    BigDecimal::from_str(&cleaned).ok()
}

impl RowExt for PgRow {
    fn to_json(&self) -> Value {
        let columns = self.columns();
        let mut map = Map::with_capacity(columns.len());

        for column in columns {
            let idx = column.ordinal();
            let type_name = column.type_info().name().to_ascii_uppercase();

            let value = if self.try_get_raw(idx).is_ok_and(|v| v.is_null()) {
                Value::Null
            } else {
                match type_name.as_str() {
                    "BOOL" => self.try_get::<bool, _>(idx).map_or(Value::Null, Value::Bool),

                    "INT8" => self
                        .try_get::<i64, _>(idx)
                        .map_or(Value::Null, |v| Value::Number(v.into())),

                    "INT4" | "OID" => self
                        .try_get::<i32, _>(idx)
                        .map_or(Value::Null, |v| Value::Number(i64::from(v).into())),

                    "INT2" => self
                        .try_get::<i16, _>(idx)
                        .map_or(Value::Null, |v| Value::Number(i64::from(v).into())),

                    "NUMERIC" => self
                        .try_get::<BigDecimal, _>(idx)
                        .map_or(Value::Null, |v| bigdecimal_to_json(&v)),

                    // dbmcp passes raw `&str` queries → sqlx uses Postgres' simple-query
                    // (text) protocol, where `PgMoney` errors out (binary-only). Parse the
                    // locale-formatted text form ($1,234.56, -$99.99) directly — assumes the
                    // en_US.UTF-8 lc_monetary default (`$` symbol, `.` decimal).
                    "MONEY" => self
                        .try_get_raw(idx)
                        .ok()
                        .and_then(|v| v.as_str().ok())
                        .and_then(parse_pg_money_text)
                        .map_or(Value::Null, |bd| bigdecimal_to_json(&bd)),

                    "FLOAT4" => self.try_get::<f32, _>(idx).map_or(Value::Null, Value::from),

                    "FLOAT8" => self.try_get::<f64, _>(idx).map_or(Value::Null, Value::from),

                    "BYTEA" => self
                        .try_get::<Vec<u8>, _>(idx)
                        .map_or(Value::Null, |bytes| Value::String(BASE64.encode(&bytes))),

                    "JSON" | "JSONB" => self.try_get::<Value, _>(idx).unwrap_or(Value::Null),

                    "DATE" => self
                        .try_get::<NaiveDate, _>(idx)
                        .map_or(Value::Null, |v| Value::String(v.to_string())),

                    "TIME" => self
                        .try_get::<NaiveTime, _>(idx)
                        .map_or(Value::Null, |v| Value::String(v.to_string())),

                    "TIMESTAMP" => self
                        .try_get::<NaiveDateTime, _>(idx)
                        .map_or(Value::Null, |v| Value::String(format!("{}T{}", v.date(), v.time()))),

                    "TIMESTAMPTZ" => self.try_get::<DateTime<Utc>, _>(idx).map_or(Value::Null, |v| {
                        let n = v.naive_utc();
                        Value::String(format!("{}T{}Z", n.date(), n.time()))
                    }),

                    _ => self.try_get::<String, _>(idx).map_or(Value::Null, Value::String),
                }
            };

            map.insert(column.name().to_string(), value);
        }

        Value::Object(map)
    }
}
