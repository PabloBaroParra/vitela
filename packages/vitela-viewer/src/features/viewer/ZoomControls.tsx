// The viewer's toolbar: zoom buttons, the zoom level and the current page.
// Presentational.

interface ZoomControlsProps {
  readonly zoom: number;
  readonly page: number;
  readonly pageCount: number;
  readonly onZoomIn: () => void;
  readonly onZoomOut: () => void;
  readonly onFitWidth: () => void;
  readonly canZoomIn: boolean;
  readonly canZoomOut: boolean;
}

export function ZoomControls(props: ZoomControlsProps) {
  const { zoom, page, pageCount, onZoomIn, onZoomOut, onFitWidth, canZoomIn, canZoomOut } = props;
  return (
    <div className="viewer-toolbar" role="toolbar" aria-label="View">
      <button type="button" onClick={onZoomOut} disabled={!canZoomOut} aria-label="Zoom out">
        −
      </button>
      <output className="viewer-toolbar__zoom" aria-label="Zoom level">{`${Math.round(zoom * 100)}%`}</output>
      <button type="button" onClick={onZoomIn} disabled={!canZoomIn} aria-label="Zoom in">
        +
      </button>
      <button type="button" onClick={onFitWidth}>
        Fit width
      </button>
      <span className="viewer-toolbar__page" aria-live="polite">{`Page ${page} of ${pageCount}`}</span>
    </div>
  );
}
