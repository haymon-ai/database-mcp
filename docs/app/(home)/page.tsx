import { Hero } from '@/components/homepage/hero';
import { Databases } from '@/components/homepage/databases';
import { Agents } from '@/components/homepage/agents';

export default function HomePage() {
  return (
    <div className="flex flex-col flex-1 w-full bg-white">
      <Hero />
      <hr className="mx-auto w-full max-w-4xl border-black/[0.08]" />
      <Databases />
      <hr className="mx-auto w-full max-w-4xl border-black/[0.08]" />
      <Agents />
    </div>
  );
}
