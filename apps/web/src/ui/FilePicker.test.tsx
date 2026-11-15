import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { PickedFile, WritableFileHandle } from '../platform/fileHandle';
import { FilePicker } from './FilePicker';

afterEach(cleanup);

const file = new File([new Uint8Array([37])], 'a.pdf', { type: 'application/pdf' });

describe('FilePicker', () => {
  it('reports a file chosen through the input', () => {
    const onPick = vi.fn();
    render(<FilePicker label="Open PDF" accept=".pdf" onPick={onPick} />);
    fireEvent.change(screen.getByLabelText('Open PDF'), { target: { files: [file] } });
    expect(onPick).toHaveBeenCalledWith(file);
  });

  it('uses the browser file picker instead of the input when one is given, and passes the handle on', async () => {
    const handle = { name: 'a.pdf' } as WritableFileHandle;
    const pick = vi.fn(async (): Promise<PickedFile | null> => ({ file, handle }));
    const onPick = vi.fn();
    render(<FilePicker label="Open PDF" accept=".pdf" onPick={onPick} pick={pick} />);

    // fireEvent returns false when the click's default (the native dialog) was prevented.
    expect(fireEvent.click(screen.getByLabelText('Open PDF'))).toBe(false);
    await waitFor(() => expect(onPick).toHaveBeenCalledWith(file, handle));
  });

  it('does nothing when the user dismisses the browser picker', async () => {
    const pick = vi.fn(async () => null);
    const onPick = vi.fn();
    render(<FilePicker label="Open PDF" accept=".pdf" onPick={onPick} pick={pick} />);
    fireEvent.click(screen.getByLabelText('Open PDF'));
    await waitFor(() => expect(pick).toHaveBeenCalled());
    expect(onPick).not.toHaveBeenCalled();
  });

  it('does nothing when the browser picker is aborted', async () => {
    const pick = vi.fn(async (): Promise<PickedFile | null> => {
      throw new DOMException('dismissed', 'AbortError');
    });
    const onPick = vi.fn();
    render(<FilePicker label="Open PDF" accept=".pdf" onPick={onPick} pick={pick} />);
    const clicks = recordClicks();
    fireEvent.click(screen.getByLabelText('Open PDF'));
    await waitFor(() => expect(pick).toHaveBeenCalled());
    await Promise.resolve();
    expect(clicks).toEqual([true]);
    expect(onPick).not.toHaveBeenCalled();
  });

  it('opens the native dialog for that click only when the browser picker fails, then uses the picker again', async () => {
    const pick = vi.fn(async (): Promise<PickedFile | null> => {
      throw new Error('blocked');
    });
    render(<FilePicker label="Open PDF" accept=".pdf" onPick={vi.fn()} pick={pick} />);
    const input = screen.getByLabelText('Open PDF');
    const clicks = recordClicks();

    fireEvent.click(input);
    // The first click was intercepted; the fallback click right after it was not.
    await waitFor(() => expect(clicks).toEqual([true, false]));

    fireEvent.click(input);
    await waitFor(() => expect(pick).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(clicks).toEqual([true, false, true, false]));
  });
});

/** The `defaultPrevented` of every click that reaches the document, in order. */
function recordClicks(): boolean[] {
  const clicks: boolean[] = [];
  document.addEventListener('click', (event) => clicks.push(event.defaultPrevented));
  return clicks;
}
