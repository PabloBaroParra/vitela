// Undo and redo in the viewer's toolbar. Presentational.
//
// `aria-disabled`, not `disabled`: a button that turns disabled under the
// keyboard focus (Undo after the last undo) would drop the focus to the page.

import { historyShortcuts, isApplePlatform } from '../document/historyKeys';

interface HistoryControlsProps {
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  readonly onUndo: () => void;
  readonly onRedo: () => void;
}

export function HistoryControls({ canUndo, canRedo, onUndo, onRedo }: HistoryControlsProps) {
  const shortcuts = historyShortcuts(isApplePlatform());
  return (
    <div className="viewer-toolbar history-controls" role="toolbar" aria-label="History">
      <button type="button" aria-disabled={!canUndo} aria-keyshortcuts={shortcuts.undo} onClick={canUndo ? onUndo : undefined}>
        Undo
      </button>
      <button type="button" aria-disabled={!canRedo} aria-keyshortcuts={shortcuts.redo} onClick={canRedo ? onRedo : undefined}>
        Redo
      </button>
    </div>
  );
}
