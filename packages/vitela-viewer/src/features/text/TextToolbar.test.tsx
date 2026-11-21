import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { IDLE } from '../search/searchState';
import { TextToolbar, type MarkupControls } from './TextToolbar';

afterEach(cleanup);

function toolbar(markup: Partial<MarkupControls> = {}) {
  const onMark = vi.fn();
  const view = render(
    <TextToolbar
      allowed
      search={IDLE}
      onSubmit={() => {}}
      onNext={() => {}}
      onPrevious={() => {}}
      onClear={() => {}}
      canCopy
      onCopy={() => {}}
      copyStatus="idle"
      markup={{ allowed: true, canMark: true, onMark, ...markup }}
    />,
  );
  return { ...view, onMark };
}

describe('TextToolbar markup', () => {
  it('marks the selection up as highlight, underline or strikeout', () => {
    const { getByRole, onMark } = toolbar();
    fireEvent.click(getByRole('button', { name: 'Highlight' }));
    fireEvent.click(getByRole('button', { name: 'Underline' }));
    fireEvent.click(getByRole('button', { name: 'Strikeout' }));
    expect(onMark.mock.calls).toEqual([['highlight'], ['underline'], ['strikeout']]);
  });

  it('stays focusable but does nothing without a selection', () => {
    const { getByRole, onMark } = toolbar({ canMark: false });
    const highlight = getByRole('button', { name: 'Highlight' });
    expect(highlight).toHaveProperty('disabled', false);
    expect(highlight.getAttribute('aria-disabled')).toBe('true');
    fireEvent.click(highlight);
    expect(onMark).not.toHaveBeenCalled();
  });

  it('explains why markup is off when the PDF does not allow annotations', () => {
    const { getByRole, onMark } = toolbar({ allowed: false });
    for (const name of ['Highlight', 'Underline', 'Strikeout']) {
      const button = getByRole('button', { name });
      expect(button.getAttribute('aria-disabled')).toBe('true');
      const note = document.getElementById(button.getAttribute('aria-describedby')!);
      expect(note?.textContent).toBe('This PDF does not allow annotations, so Highlight, Underline and Strikeout are turned off.');
      fireEvent.click(button);
    }
    expect(onMark).not.toHaveBeenCalled();
  });
});
