// One tool in the grid. The whole card is a link that opens the tool in a
// new tab.

import type { CSSProperties } from 'react';
import { ToolIcon } from './ToolIcon';

interface ToolCardProps {
  readonly href: string;
  readonly name: string;
  readonly description: string;
  readonly icon: readonly string[];
  readonly tone: string;
  /** False while the tool is only in the apps. */
  readonly available: boolean;
  /** Position in the grid, for the staggered entrance. */
  readonly index: number;
}

export function ToolCard({ href, name, description, icon, tone, available, index }: ToolCardProps) {
  return (
    <li className="tool-card" style={{ '--i': index } as CSSProperties}>
      <a className="tool-card__link" href={href} target="_blank" rel="noopener">
        <span className="tool-card__top">
          <ToolIcon icon={icon} tone={tone} />
          <span className={available ? 'tool-card__badge tool-card__badge--live' : 'tool-card__badge'}>
            {available ? 'Available' : 'Coming soon'}
          </span>
        </span>
        <h3 className="tool-card__name">{name}</h3>
        <p className="tool-card__description">{description}</p>
        <span className="visually-hidden"> (opens in a new tab)</span>
      </a>
    </li>
  );
}
