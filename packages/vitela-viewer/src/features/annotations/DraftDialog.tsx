// The dialog of a note or text box waiting for its text: the note's plain
// text area, or the text box's with the core's layout previewed under it.

import type { Engine } from '@vitela/core';
import { FreeTextDialog } from './FreeTextDialog';
import { TextDialog } from './TextDialog';
import type { Draft } from './useDrawing';

interface DraftDialogProps {
  readonly engine: Engine;
  readonly draft: Draft;
  readonly onChange: () => void;
  readonly onSubmit: (text: string) => void;
  readonly onCancel: () => void;
}

export function DraftDialog({ engine, draft, onChange, onSubmit, onCancel }: DraftDialogProps) {
  const shared = { busy: draft.busy, error: draft.error, onChange, onSubmit, onCancel };
  if (draft.tool === 'note') return <TextDialog title="Add a note" label="Note" submit="Add note" {...shared} />;
  return <FreeTextDialog engine={engine} title="Add a text box" submit="Add text box" frame={draft.frame} {...shared} />;
}
