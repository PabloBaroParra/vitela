// The Organize screen's toolbar: the Pages / Documents view, Add PDFs, Add
// blank page (an A4 page, portrait or landscape — what the native shells
// offer — after the selection), what can be done to the selection, Extract
// (the selection) and Split when the workspace offers them, and undo/redo.
// Presentational.
//
// The buttons are `aria-disabled`, not `disabled`: a button that turns
// disabled under the keyboard focus (Undo after the last undo) would drop the
// focus to the page.

import type { PageOrientation } from '@vitela/core';
import { FilePicker } from '../../ui/FilePicker';
import { MenuButton } from '../../ui/MenuButton';
import { historyShortcuts, isApplePlatform } from '../document/historyKeys';
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
  /**
   * Takes the focus when pressed, as every browser but Safari does on its
   * own: a dialog it opens gives the focus back to what had it.
   */
  readonly opensDialog?: boolean;
}

function ToolButton({ label, shortcut, enabled, onPress, opensDialog = false }: ToolButtonProps) {
  return (
    <button
      type="button"
      aria-disabled={!enabled}
      aria-keyshortcuts={shortcut}
      onClick={
        enabled
          ? (event) => {
              if (opensDialog) event.currentTarget.focus();
              onPress();
            }
          : undefined
      }
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
  /** Add blank page is on: the document permits it and no edit is in flight. */
  readonly canInsert: boolean;
  readonly canMoveEarlier: boolean;
  readonly canMoveLater: boolean;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  /** Extract is on: there is a selection and no extract or split is running. */
  readonly canExtract?: boolean;
  /** Split is on: no extract or split is running. */
  readonly canSplit?: boolean;
  readonly onInsertBlank: (orientation: PageOrientation) => void;
  /** Opens Extract pages on the selection; no Extract button without it. */
  readonly onExtract?: () => void;
  /** Opens Split; no Split button without it. */
  readonly onSplit?: () => void;
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
  const shortcuts = historyShortcuts(isApplePlatform());
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
      <MenuButton
        label="Add blank page"
        className="organize__insert"
        disabled={!props.canInsert}
        tools={[
          { label: 'Portrait A4', onSelect: () => props.onInsertBlank('portrait') },
          { label: 'Landscape A4', onSelect: () => props.onInsertBlank('landscape') },
        ]}
      />
      <ToolButton label="Rotate left" enabled={canEdit} onPress={props.onRotateLeft} />
      <ToolButton label="Rotate right" enabled={canEdit} onPress={props.onRotateRight} />
      <ToolButton label="Delete" shortcut="Delete" enabled={canEdit} onPress={props.onDelete} />
      {props.onExtract && <ToolButton label="Extract" enabled={props.canExtract ?? false} onPress={props.onExtract} opensDialog />}
      {props.onSplit && <ToolButton label="Split" enabled={props.canSplit ?? false} onPress={props.onSplit} opensDialog />}
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
      <ToolButton label="Undo" shortcut={shortcuts.undo} enabled={canUndo} onPress={props.onUndo} />
      <ToolButton label="Redo" shortcut={shortcuts.redo} enabled={canRedo} onPress={props.onRedo} />
      <span className="viewer-toolbar__page" aria-live="polite">
        {summary}
      </span>
    </div>
  );
}
