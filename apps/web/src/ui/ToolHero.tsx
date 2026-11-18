// The hero of a single tool's page: its icon tile, name and description,
// then whatever the container puts below (the open action, or the "coming
// to the web" note).

import type { CSSProperties, ReactNode } from 'react';
import { ToolIcon } from './ToolIcon';

interface ToolHeroProps {
  readonly name: string;
  readonly description: string;
  readonly icon: readonly string[];
  readonly tone: string;
  /** Shown above the name, e.g. a "Coming to the web" badge. */
  readonly badge?: ReactNode;
  readonly children: ReactNode;
}

const step = (i: number) => ({ '--i': i }) as CSSProperties;

export function ToolHero({ name, description, icon, tone, badge, children }: ToolHeroProps) {
  return (
    <section className="tool-hero" aria-labelledby="tool-title">
      <div className="rise" style={step(0)}>
        <ToolIcon icon={icon} tone={tone} size="lg" />
      </div>
      {badge && (
        <div className="tool-hero__badge rise" style={step(1)}>
          {badge}
        </div>
      )}
      <h2 id="tool-title" className="tool-hero__title rise" style={step(1)}>
        {name}
      </h2>
      <p className="tool-hero__lead rise" style={step(2)}>
        {description}
      </p>
      <div className="tool-hero__body rise" style={step(3)}>
        {children}
      </div>
    </section>
  );
}
