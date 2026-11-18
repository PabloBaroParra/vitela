// The viewer's text toolbar: find in the document, step between matches,
// copy the selection and mark it up (Highlight, Underline, Strikeout) — or,
// for a document that withholds its text or forbids annotations, why that is
// not available. Presentational apart from the query being typed.
//
// The markup buttons are `aria-disabled`, not `disabled`: a button that turns
// disabled under the keyboard focus (the selection is dropped once marked)
// would drop the focus to the page.

import { useId, useState } from 'react';
import type { MarkupKind } from '@vitela/core';
import type { SearchState } from '../search/searchState';
import { summary } from '../search/searchState';
import type { CopyStatus } from './useCopy';

interface TextToolbarProps {
  /** `false` when the document withholds its text: everything is disabled. */
  readonly allowed: boolean;
  readonly search: SearchState;
  /** Enter (or Shift+Enter, `backwards`) in the box. */
  readonly onSubmit: (query: string, backwards: boolean) => void;
  readonly onNext: () => void;
  readonly onPrevious: () => void;
  readonly onClear: () => void;
  readonly canCopy: boolean;
  readonly onCopy: () => void;
  readonly copyStatus: CopyStatus;
  readonly markup: MarkupControls;
}

/** The markup actions over the text selection. */
export interface MarkupControls {
  /** `false` when the document forbids annotations: the buttons say why. */
  readonly allowed: boolean;
  /** There is a selection to mark and no edit, undo or redo is still running. */
  readonly canMark: boolean;
  readonly onMark: (kind: MarkupKind) => void;
}

const MARKUP: readonly { readonly kind: MarkupKind; readonly label: string }[] = [
  { kind: 'highlight', label: 'Highlight' },
  { kind: 'underline', label: 'Underline' },
  { kind: 'strikeout', label: 'Strikeout' },
];

const COPY_STATUS: Record<CopyStatus, string> = {
  idle: '',
  copied: 'Copied',
  failed: 'Could not copy to the clipboard',
};

export function TextToolbar(props: TextToolbarProps) {
  const { allowed, search, onSubmit, onNext, onPrevious, onClear, canCopy, onCopy, copyStatus, markup } = props;
  const [query, setQuery] = useState('');
  const blockedId = useId();
  const markupBlockedId = useId();
  const canMark = allowed && markup.allowed && markup.canMark;
  const canStep = search.kind === 'found' && search.hits.length > 0;

  return (
    <div className="viewer-toolbar text-toolbar" role="toolbar" aria-label="Text">
      <form
        className="text-toolbar__search"
        role="search"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit(query, false);
        }}
      >
        <input
          type="search"
          aria-label="Find in document"
          placeholder="Find (exact, case-sensitive)"
          value={query}
          disabled={!allowed}
          aria-describedby={allowed ? undefined : blockedId}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && event.shiftKey) {
              event.preventDefault();
              onSubmit(query, true);
            } else if (event.key === 'Escape') {
              setQuery('');
              onClear();
            }
          }}
        />
        <button type="button" onClick={onPrevious} disabled={!canStep} aria-label="Previous match">
          ↑
        </button>
        <button type="button" onClick={onNext} disabled={!canStep} aria-label="Next match">
          ↓
        </button>
        <output className="text-toolbar__result" aria-label="Search results" aria-live="polite">
          {summary(search)}
        </output>
      </form>
      <button type="button" onClick={onCopy} disabled={!allowed || !canCopy}>
        Copy
      </button>
      <span className="text-toolbar__copied" aria-live="polite">
        {COPY_STATUS[copyStatus]}
      </span>
      <span className="text-toolbar__markup" role="group" aria-label="Mark up the selection">
        {MARKUP.map(({ kind, label }) => (
          <button
            key={kind}
            type="button"
            className={`markup-button markup-button--${kind}`}
            aria-disabled={!canMark}
            aria-describedby={markup.allowed ? undefined : markupBlockedId}
            onClick={canMark ? () => markup.onMark(kind) : undefined}
          >
            {label}
          </button>
        ))}
      </span>
      {!allowed && (
        <p id={blockedId} className="text-toolbar__blocked" role="note">
          This PDF does not allow copying its text, so search and text selection are turned off.
        </p>
      )}
      {!markup.allowed && (
        <p id={markupBlockedId} className="text-toolbar__blocked" role="note">
          This PDF does not allow annotations, so Highlight, Underline and Strikeout are turned off.
        </p>
      )}
    </div>
  );
}
