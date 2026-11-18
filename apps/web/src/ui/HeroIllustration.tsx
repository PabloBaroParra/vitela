// The home hero picture: a stylised PDF page with a search hit and an image,
// plus a few tool tiles and a "Saved" pill floating around it. Pure SVG drawn
// with theme tokens, so it follows light and dark and fetches nothing.

const EDIT_ICON = ['M3.5 20.5l1.2-4.9L15.6 4.3a2.1 2.1 0 0 1 3 0l1.1 1.1a2.1 2.1 0 0 1 0 3L8.4 19.3Z', 'M14.6 5.3 18.7 9.4'];
const ORGANIZE_ICON = [
  'M5 3.5h4A1.5 1.5 0 0 1 10.5 5v4A1.5 1.5 0 0 1 9 10.5H5A1.5 1.5 0 0 1 3.5 9V5A1.5 1.5 0 0 1 5 3.5Z',
  'M15 3.5h4A1.5 1.5 0 0 1 20.5 5v4a1.5 1.5 0 0 1-1.5 1.5h-4A1.5 1.5 0 0 1 13.5 9V5A1.5 1.5 0 0 1 15 3.5Z',
  'M5 13.5h4a1.5 1.5 0 0 1 1.5 1.5v4A1.5 1.5 0 0 1 9 20.5H5A1.5 1.5 0 0 1 3.5 19v-4A1.5 1.5 0 0 1 5 13.5Z',
  'M15 13.5h4a1.5 1.5 0 0 1 1.5 1.5v4a1.5 1.5 0 0 1-1.5 1.5h-4a1.5 1.5 0 0 1-1.5-1.5v-4a1.5 1.5 0 0 1 1.5-1.5Z',
];

/** Text lines of the page: [x, y, width]. */
const LINES: readonly (readonly [number, number, number])[] = [
  [134, 96, 142],
  [134, 112, 128],
  [134, 128, 138],
  [134, 144, 96],
  [226, 186, 50],
  [226, 202, 44],
  [226, 218, 50],
  [134, 254, 136],
];

function Tile({ x, y, size, tone, icon }: { x: number; y: number; size: number; tone: string; icon: readonly string[] }) {
  const scale = (size * 0.46) / 24;
  const offset = (size - 24 * scale) / 2;
  return (
    <g className={`tone-${tone}`}>
      <rect className="illo__tile" x={x} y={y} width={size} height={size} rx={size * 0.26} />
      <g className="illo__tile-icon" transform={`translate(${x + offset} ${y + offset}) scale(${scale})`}>
        {icon.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
    </g>
  );
}

export function HeroIllustration() {
  return (
    <div className="hero-illo" aria-hidden="true">
      <svg className="hero-illo__svg" viewBox="0 0 400 340" width="400" height="340">
        <g className="illo__float illo__float--slow">
          <Tile x={44} y={150} size={58} tone="green" icon={ORGANIZE_ICON} />
        </g>

        <g className="illo__float">
          <rect className="illo__page" x={112} y={34} width={186} height={252} rx={10} />
          <rect className="illo__heading" x={134} y={62} width={92} height={11} rx={5.5} />
          <rect className="illo__hit" x={192} y={124} width={58} height={15} rx={4} />
          {LINES.map(([x, y, width]) => (
            <rect key={`${x}-${y}`} className="illo__line" x={x} y={y} width={width} height={7} rx={3.5} />
          ))}
          <g className="tone-blue">
            <rect className="illo__tile" x={134} y={174} width={80} height={60} rx={6} />
            <path className="illo__picture" d="m140 226 18-20 12 12 9-8 25 16" />
            <circle className="illo__picture" cx={190} cy={190} r={5} />
          </g>
        </g>

        <g className="illo__float illo__float--late">
          <Tile x={280} y={52} size={64} tone="violet" icon={EDIT_ICON} />
        </g>

        <g className="illo__float illo__float--slow">
          <rect className="illo__pill" x={236} y={262} width={112} height={38} rx={19} />
          <circle className="illo__pill-dot" cx={258} cy={281} r={9} />
          <path className="illo__pill-check" d="m253.5 281 3 3 5.5-6" />
          <text className="illo__pill-text" x={274} y={286}>
            Saved
          </text>
        </g>
      </svg>
    </div>
  );
}
