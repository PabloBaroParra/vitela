// The home page hero: headline, the open action and the illustration.
// Presentational: the container hands in the action (the picker, or a status
// line while the engine loads).

import type { CSSProperties, ReactNode } from 'react';
import { HeroIllustration } from './HeroIllustration';

interface HomeHeroProps {
  /** The open control, or what stands in for it. */
  readonly action: ReactNode;
}

const step = (i: number) => ({ '--i': i }) as CSSProperties;

export function HomeHero({ action }: HomeHeroProps) {
  return (
    <section className="hero" aria-labelledby="hero-title">
      <div className="hero__text">
        <h2 id="hero-title" className="hero__title rise" style={step(0)}>
          Work with PDFs, right in your browser
        </h2>
        <p className="hero__lead rise" style={step(1)}>
          Open, search, copy and save PDF files. Everything runs on your device, so your files are never uploaded.
        </p>
        <div className="hero__action rise" style={step(2)}>
          {action}
        </div>
        <p className="hero__drop rise" style={step(3)}>
          or drop a PDF anywhere on this page
        </p>
      </div>
      <div className="hero__art rise" style={step(4)}>
        <HeroIllustration />
      </div>
    </section>
  );
}
