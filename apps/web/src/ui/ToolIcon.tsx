// A tool's icon on a tinted tile. `tone` picks the hue (`--tone-<tone>-*`).

interface ToolIconProps {
  /** SVG path data (`d`) of each stroke, on a 24×24 grid. */
  readonly icon: readonly string[];
  readonly tone: string;
  readonly size?: 'md' | 'lg';
}

export function ToolIcon({ icon, tone, size = 'md' }: ToolIconProps) {
  return (
    <span className={`tool-icon tool-icon--${size} tone-${tone}`} aria-hidden="true">
      <svg viewBox="0 0 24 24" width="24" height="24">
        {icon.map((d) => (
          <path key={d} d={d} />
        ))}
      </svg>
    </span>
  );
}
