// The comments of the document by page: those the file carries and the
// session's notes. Each one can be gone to or read; none is edited here.
// Presentational.

import { useRef } from 'react';
import { focusing } from '../annotations/focusing';
import { useKeepFocus } from '../annotations/useKeepFocus';
import { commentByline } from './commentText';
import type { ReadableNote } from './NoteDialog';
import type { CommentEntry } from './stops';
import type { CommentsState } from './useComments';

interface CommentsPanelProps {
  readonly id: string;
  readonly state: CommentsState;
  readonly entries: readonly CommentEntry[];
  /** The one Previous/Next is on. */
  readonly currentKey: string | null;
  readonly onGo: (key: string) => void;
  readonly onRead: (note: ReadableNote) => void;
  readonly onRetry: () => void;
  readonly onClose: () => void;
}

export function CommentsPanel({ id, state, entries, currentKey, onGo, onRead, onRetry, onClose }: CommentsPanelProps) {
  const panel = useRef<HTMLElement>(null);
  const focusPanel = () => panel.current?.focus({ preventScroll: true });
  return (
    <aside
      ref={panel}
      id={id}
      className="comments-panel"
      aria-label="Comments"
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key !== 'Escape' || event.defaultPrevented) return;
        event.preventDefault();
        onClose();
      }}
    >
      <div className="comments-panel__head">
        <h2 className="comments-panel__title">Comments</h2>
        <button type="button" aria-label="Close comments" onClick={onClose}>
          ×
        </button>
      </div>
      {state.status === 'loading' && (
        <p className="comments-panel__note" role="status">
          Loading comments…
        </p>
      )}
      {state.status === 'failed' && <Failure message={state.message} onRetry={onRetry} onFocusLost={focusPanel} />}
      {state.status === 'ready' && entries.length === 0 && <p className="comments-panel__note">This document has no comments.</p>}
      {byPage(entries).map(([pageIndex, onPage]) => (
        <section key={pageIndex}>
          <h3 className="comments-panel__page">{`Page ${pageIndex + 1}`}</h3>
          <ul className="comments-panel__list">
            {onPage.map((entry) => (
              <Row
                key={entry.key}
                entry={entry}
                current={entry.key === currentKey}
                onGo={onGo}
                onRead={onRead}
                onFocusLost={focusPanel}
              />
            ))}
          </ul>
        </section>
      ))}
    </aside>
  );
}

interface RowProps extends Pick<CommentsPanelProps, 'onGo' | 'onRead'> {
  readonly entry: CommentEntry;
  readonly current: boolean;
  readonly onFocusLost: () => void;
}

function Row({ entry: { key, comment }, current, onGo, onRead, onFocusLost }: RowProps) {
  const row = useRef<HTMLLIElement>(null);
  useKeepFocus(row, onFocusLost);
  const byline = commentByline(comment);
  return (
    <li ref={row} className="comment-row">
      <button type="button" className="comment-row__go" aria-current={current || undefined} onClick={focusing(() => onGo(key))}>
        <span className="comment-row__text">{comment.contents}</span>
        {byline && <span className="comment-row__byline">{byline}</span>}
      </button>
      <button
        type="button"
        className="comment-row__read"
        aria-label={`Read note on page ${comment.pageIndex + 1}: ${excerpt(comment.contents)}`}
        onClick={focusing(() => onRead(comment))}
      >
        Read note
      </button>
    </li>
  );
}

interface FailureProps {
  readonly message: string;
  readonly onRetry: () => void;
  readonly onFocusLost: () => void;
}

function Failure({ message, onRetry, onFocusLost }: FailureProps) {
  const block = useRef<HTMLDivElement>(null);
  useKeepFocus(block, onFocusLost);
  return (
    <div ref={block} className="comments-panel__note">
      <p className="comments-panel__error" role="alert">{`Could not list the comments: ${message}`}</p>
      <button type="button" onClick={focusing(onRetry)}>
        Try again
      </button>
    </div>
  );
}

const EXCERPT_LENGTH = 60;

/** The start of a comment, on one line. */
function excerpt(contents: string): string {
  const line = contents.trim().replace(/\s+/g, ' ');
  return line.length > EXCERPT_LENGTH ? `${line.slice(0, EXCERPT_LENGTH)}…` : line;
}

/** The entries of each page, in the order they come (the core's: by page). */
function byPage(entries: readonly CommentEntry[]): [number, CommentEntry[]][] {
  const pages = new Map<number, CommentEntry[]>();
  for (const entry of entries) {
    const list = pages.get(entry.comment.pageIndex);
    if (list) list.push(entry);
    else pages.set(entry.comment.pageIndex, [entry]);
  }
  return [...pages];
}
