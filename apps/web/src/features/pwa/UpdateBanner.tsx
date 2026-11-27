// Presentational: a new version of the app is installed and waiting. The
// reload is the user's, and is held while there are unsaved changes.

interface UpdateBannerProps {
  /** The open document has unsaved changes: a reload would lose them. */
  readonly held: boolean;
  readonly onReload: () => void;
}

export function UpdateBanner({ held, onReload }: UpdateBannerProps) {
  return (
    <div className="update-banner" role="status">
      <span>
        A new version of Vitela is ready.{held ? ' Save your changes to reload into it.' : ''}
      </span>
      <button type="button" className="button--primary update-banner__reload" onClick={onReload} disabled={held}>
        Reload
      </button>
    </div>
  );
}
