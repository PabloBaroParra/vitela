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

  it('lets the next click open the native input dialog once the browser picker failed', async () => {
    const pick = vi.fn(async (): Promise<PickedFile | null> => {
      throw new Error('blocked');
    });
    render(<FilePicker label="Open PDF" accept=".pdf" onPick={vi.fn()} pick={pick} />);
    const input = screen.getByLabelText('Open PDF');
    expect(fireEvent.click(input)).toBe(false);
    await waitFor(() => expect(pick).toHaveBeenCalled());
    await waitFor(() => expect(fireEvent.click(input)).toBe(true));
    expect(pick).toHaveBeenCalledTimes(1);
  });
});
