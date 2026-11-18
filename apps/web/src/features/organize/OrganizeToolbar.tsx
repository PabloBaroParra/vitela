// The Organize screen's toolbar: the Pages / Documents view, Add PDFs, what
// can be done to the selection, and undo/redo. Presentational.
//
// The buttons are `aria-disabled`, not `disabled`: a button that turns
// disabled under the keyboard focus (Undo after the last undo) would drop the
// focus to the page.

import { FilePicker } from '../../ui/FilePicker';
import { ViewSwitch } from '../../ui/ViewSwitch';
import type { OrganizeView } from './units';

const VIEWS = [
  { value: 'pages', label: 'Pages' },
  { value: 'documents', label: 'Documents' },
] as const;

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
  readonly view: OrganizeView;
  readonly onViewChange: (view: OrganizeView) => void;
  /** Why Add PDFs is off (the core's import refusal), or `null` when it is on. */
  readonly addRefusal: string | null;
  /** Add PDFs is busy with a pick. */
  readonly adding: boolean;
  readonly onAddFiles: (files: File[]) => void;
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
      <ViewSwitch label="Organize by" views={VIEWS} current={props.view} onChange={props.onViewChange} />
      <FilePicker
        label="Add PDFs"
        accept="application/pdf,.pdf"
        onPickFiles={props.onAddFiles}
        disabled={props.addRefusal !== null || props.adding}
        disabledReason={props.addRefusal ?? undefined}
      />
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
