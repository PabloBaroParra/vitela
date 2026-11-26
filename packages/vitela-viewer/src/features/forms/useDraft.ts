// Text typed into a field before it is filled in. The field shows the
// document's value until something is typed; the draft is sent when the field
// is left (or on Enter), and only if it differs — from the text still being
// sent, while one is (putting the old text back then is a fill of its own),
// else from the document's value. A draft the core refused stays,
// so it can be corrected; Escape puts the document's value back. Container
// logic only.

import { useCallback, useRef, useState } from 'react';

export function useDraft(current: string, readOnly: boolean, commit: (text: string) => Promise<boolean>) {
  const [draft, setDraft] = useState<string | null>(null);
  /** The last text sent whose fill has not ended yet. */
  const sending = useRef<{ readonly text: string } | null>(null);

  const change = useCallback((text: string) => !readOnly && setDraft(text), [readOnly]);

  /** Sends the draft, if there is one that differs; settles once it has ended. */
  const send = useCallback(async () => {
    if (draft === null || readOnly) return;
    if (draft === (sending.current?.text ?? current)) {
      // A draft still being sent stays until its step lands.
      if (!sending.current) setDraft(null);
      return;
    }
    const text = draft;
    const token = { text };
    sending.current = token;
    let filled: boolean;
    try {
      filled = await commit(text);
    } finally {
      if (sending.current === token) sending.current = null;
    }
    // Kept until the field holds it: once the step lands, `current` is the text.
    if (filled) setDraft((shown) => (shown === text ? null : shown));
  }, [draft, current, readOnly, commit]);

  /** Drops a draft that differs from the document's value; `false` when there was none. */
  const revert = useCallback(() => {
    if (draft === null) return false;
    setDraft(null);
    return draft !== current;
  }, [draft, current]);

  return { value: draft ?? current, change, send, revert };
}
