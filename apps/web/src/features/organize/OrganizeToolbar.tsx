// The Organize screen's toolbar: what can be done to the selected pages, and
// undo/redo. Presentational.
//
// The buttons are `aria-disabled`, not `disabled`: a button that turns
// disabled under the keyboard focus (Undo after the last undo) would drop the
// focus to the page.

interface ToolButtonProps {
  readonly label: string;
  readonly shortcut?: string;
  readonly enabled: boolean;
  readonly onPress: () => void;
}

function ToolButton({ label, shortcut, enabled, onPress }: ToolButtonProps) {
  return (
    <button
      type="button"
      aria-disabled={!enabled}
      aria-keyshortcuts={shortcut}
      onClick={enabled ? onPress : undefined}
    >
      {label}
    </button>
  );
}

interface OrganizeToolbarProps {
  /** The selection as text, e.g. "Pages 2–3 of 5 selected". */
  readonly summary: string;
  readonly canEdit: boolean;
  readonly canMoveEarlier: boolean;
  readonly canMoveLater: boolean;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  readonly onRotateLeft: () => void;
  readonly onRotateRight: () => void;
  readonly onDelete: () => void;
  readonly onMoveEarlier: () => void;
  readonly onMoveLater: () => void;
  readonly onUndo: () => void;
  readonly onRedo: () => void;
}

export function OrganizeToolbar(props: OrganizeToolbarProps) {
  const { summary, canEdit, canMoveEarlier, canMoveLater, canUndo, canRedo } = props;
  return (
    <div className="viewer-toolbar organize__toolbar" role="toolbar" aria-label="Organize">
      <ToolButton label="Rotate left" enabled={canEdit} onPress={props.onRotateLeft} />
      <ToolButton label="Rotate right" enabled={canEdit} onPress={props.onRotateRight} />
      <ToolButton label="Delete" shortcut="Delete" enabled={canEdit} onPress={props.onDelete} />
      <ToolButton
        label="Move earlier"
        shortcut="Control+Shift+ArrowLeft"
        enabled={canEdit && canMoveEarlier}
        onPress={props.onMoveEarlier}
      />
      <ToolButton
        label="Move later"
        shortcut="Control+Shift+ArrowRight"
        enabled={canEdit && canMoveLater}
        onPress={props.onMoveLater}
      />
      <ToolButton label="Undo" shortcut="Control+Z" enabled={canUndo} onPress={props.onUndo} />
      <ToolButton label="Redo" shortcut="Control+Shift+Z" enabled={canRedo} onPress={props.onRedo} />
      <span className="viewer-toolbar__page" aria-live="polite">
        {summary}
      </span>
    </div>
  );
}
