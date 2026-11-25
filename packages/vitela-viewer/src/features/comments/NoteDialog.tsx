// "Read note": a comment's whole text, read-only. Presentational.

import { useRef } from 'react';
import type { Comment } from '@vitela/core';
import { ModalShell } from '../annotations/ModalShell';
import { commentByline } from './commentText';

export type ReadableNote = Pick<Comment, 'pageIndex' | 'contents' | 'author' | 'date'>;

export function NoteDialog({ note, onClose }: { readonly note: ReadableNote; readonly onClose: () => void }) {
  const text = useRef<HTMLTextAreaElement>(null);
  const byline = commentByline(note);
  return (
    <ModalShell title={`Note on page ${note.pageIndex + 1}`} className="note-dialog" field={text} onCancel={onClose}>
      {(heading) => (
        <>
          {heading}
          {byline && <p className="note-dialog__byline">{byline}</p>}
          <textarea ref={text} className="note-dialog__text" aria-label="Note" rows={6} readOnly value={note.contents} />
          <div className="dialog__actions">
            <button type="button" className="button--primary" onClick={onClose}>
              Close
            </button>
          </div>
        </>
      )}
    </ModalShell>
  );
}
