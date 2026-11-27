// Presentational: a new version of the app is installed and waiting. The
// reload is the user's, and is held while there are unsaved changes, here or
// in another tab of the app.

interface UpdateBannerProps {
  /** The open document has unsaved changes: a reload would lose them. */
  readonly held: boolean;
  /** The last reload was refused: another tab of the app has unsaved changes. */
  readonly blocked?: boolean;
  readonly onReload: () => void;
}

function note(held: boolean, blocked: boolean): string {
  if (held) return ' Save your changes to reload into it.';
  if (blocked) return ' Another Vitela tab has unsaved changes: save them there, then reload.';
  return '';
}

export function UpdateBanner({ held, blocked = false, onReload }: UpdateBannerProps) {
  return (
    <div className="update-banner" role="status">
      <span>
        A new version of Vitela is ready.{note(held, blocked)}
      </span>
      <button type="button" className="button--primary update-banner__reload" onClick={onReload} disabled={held}>
        Reload
      </button>
    </div>
  );
}
