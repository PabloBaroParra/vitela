// The viewer's text toolbar: find in the document, step between matches, and
// copy the selection — or, for a document that withholds its text, why none
// of that is available. Presentational apart from the query being typed.

import { useId, useState } from 'react';
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
}

const COPY_STATUS: Record<CopyStatus, string> = {
  idle: '',
  copied: 'Copied',
  failed: 'Could not copy to the clipboard',
};

export function TextToolbar(props: TextToolbarProps) {
  const { allowed, search, onSubmit, onNext, onPrevious, onClear, canCopy, onCopy, copyStatus } = props;
  const [query, setQuery] = useState('');
  const blockedId = useId();
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
      {!allowed && (
        <p id={blockedId} className="text-toolbar__blocked" role="note">
          This PDF does not allow copying its text, so search and text selection are turned off.
        </p>
      )}
    </div>
  );
}
