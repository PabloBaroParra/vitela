// What the "Edit content" mode lays over the page of the selected text run
// (B26 T-259): the camouflaged editor while one is open, else the run's
// toolbar. Presentational wiring of `useTextRunEditing` onto its two views;
// the viewer mounts it inside that page's slot (`contentOverlay`).

import type { PageSize } from '@vitela/core';
import { TextRunEditor } from './TextRunEditor';
import { TextRunToolbar } from './TextRunToolbar';
import type { TextRunEditing } from './useTextRunEditing';

interface TextRunOverlayProps {
  readonly textRuns: TextRunEditing;
  /** CSS px per PDF point. */
  readonly scale: number;
  /** The document's pages, for the turn of the editor's page. */
  readonly pages: readonly Pick<PageSize, 'rotationDegrees'>[];
  /** Gives the keyboard back to the pages (after a toolbar button). */
  readonly focusPages: () => void;
}

/** The page the overlay goes on: the open editor's, else the selected run's; `null` for none. */
export function textRunOverlayPage(textRuns: TextRunEditing): number | null {
  return textRuns.editor?.run.pageIndex ?? textRuns.run?.pageIndex ?? null;
}

export function TextRunOverlay({ textRuns, scale, pages, focusPages }: TextRunOverlayProps) {
  const { editor, run } = textRuns;
  if (editor) {
    return (
      <TextRunEditor
        key={`${editor.pageKey}#${editor.run.id}`}
        editor={editor}
        scale={scale}
        rotationDegrees={pages[editor.run.pageIndex]?.rotationDegrees ?? 0}
        onCommit={textRuns.commit}
        onCancel={textRuns.cancel}
        onType={textRuns.typing}
        onAttach={textRuns.attach}
        onDetach={textRuns.detach}
      />
    );
  }
  if (!run) return null;
  return (
    <TextRunToolbar
      run={run}
      scale={scale}
      look={textRuns.look}
      busy={textRuns.busy}
      onEdit={textRuns.open}
      onToggle={(face) => {
        textRuns.toggle(face);
        focusPages();
      }}
      onRemove={() => {
        textRuns.remove();
        focusPages();
      }}
    />
  );
}
