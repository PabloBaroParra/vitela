// Container: the tool cards with category filter chips. Owns which category
// is shown; the cards themselves are presentational.

import { useState } from 'react';
import { ToolCard } from '../../ui/ToolCard';
import { CATEGORIES, TOOLS, type Tool, type ToolCategory } from './tools';

type Filter = ToolCategory | 'all';

interface ToolGridProps {
  readonly tools?: readonly Tool[];
}

export function ToolGrid({ tools = TOOLS }: ToolGridProps) {
  const [filter, setFilter] = useState<Filter>('all');
  // Only offer categories that have something in them.
  const chips: readonly { readonly id: Filter; readonly label: string }[] = [
    { id: 'all', label: 'All' },
    ...CATEGORIES.filter((category) => tools.some((tool) => tool.category === category.id)),
  ];
  const shown = filter === 'all' ? tools : tools.filter((tool) => tool.category === filter);

  return (
    <div className="tool-browser">
      <div className="tool-filter" role="group" aria-label="Filter tools by category">
        {chips.map((chip) => (
          <button
            key={chip.id}
            type="button"
            className="tool-filter__chip"
            aria-pressed={filter === chip.id}
            onClick={() => setFilter(chip.id)}
          >
            {chip.label}
          </button>
        ))}
      </div>
      {/* Keyed by filter: a new list replays the cards' entrance. */}
      <ul key={filter} className="tool-grid" aria-label="Tools">
        {shown.map((tool, index) => (
          <ToolCard
            key={tool.id}
            href={tool.path}
            name={tool.name}
            description={tool.description}
            icon={tool.icon}
            tone={tool.tone}
            available={tool.web}
            index={index}
          />
        ))}
      </ul>
    </div>
  );
}
