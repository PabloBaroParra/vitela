import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrawToolbar } from './DrawToolbar';
import { PALETTE, type DrawTool } from './drawing';

afterEach(cleanup);

function toolbar(props: { tool?: DrawTool; allowed?: boolean; ready?: boolean } = {}) {
  const onTool = vi.fn();
  const onColor = vi.fn();
  const onImage = vi.fn();
  const view = render(
    <DrawToolbar
      tool={props.tool ?? 'select'}
      onTool={onTool}
      color={PALETTE[0]!.color}
      onColor={onColor}
      onImage={onImage}
      allowed={props.allowed ?? true}
      ready={props.ready ?? true}
    />,
  );
  return { ...view, onTool, onColor, onImage };
}

const TOOLS = ['Ink', 'Shape', 'Note', 'Free text'];
const png = () => new File([new Uint8Array([0x89])], 'logo.png', { type: 'image/png' });

describe('DrawToolbar', () => {
  it('is a toolbar whose pressed button is the armed tool', () => {
    const { getByRole } = toolbar({ tool: 'shape' });
    expect(getByRole('toolbar', { name: 'Draw' })).toBeTruthy();
    expect(getByRole('button', { name: 'Shape' }).getAttribute('aria-pressed')).toBe('true');
    expect(getByRole('button', { name: 'Select' }).getAttribute('aria-pressed')).toBe('false');
  });

  it('arms each tool and picks a colour', () => {
    const { getByRole, onTool, onColor } = toolbar();
    for (const name of ['Select', ...TOOLS]) fireEvent.click(getByRole('button', { name }));
    expect(onTool.mock.calls.map(([tool]) => tool)).toEqual(['select', 'ink', 'shape', 'note', 'freeText']);
    expect(getByRole('button', { name: 'Yellow' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(getByRole('button', { name: 'Blue' }));
    expect(onColor).toHaveBeenCalledWith(PALETTE.find(({ name }) => name === 'Blue')!.color);
  });

  it('keeps the tools focusable but off while a step runs; Select still works', () => {
    const { getByRole, onTool } = toolbar({ ready: false });
    for (const name of TOOLS) {
      const button = getByRole('button', { name });
      expect(button).toHaveProperty('disabled', false);
      expect(button.getAttribute('aria-disabled')).toBe('true');
      fireEvent.click(button);
    }
    fireEvent.click(getByRole('button', { name: 'Select' }));
    expect(onTool.mock.calls).toEqual([['select']]);
  });

  it('explains why the tools are off when the PDF does not allow annotations', () => {
    const { getByRole, onTool } = toolbar({ allowed: false });
    for (const name of TOOLS) {
      const button = getByRole('button', { name });
      expect(button.getAttribute('aria-disabled')).toBe('true');
      const note = document.getElementById(button.getAttribute('aria-describedby')!);
      expect(note?.textContent).toBe(
        'This PDF does not allow annotations, so Ink, Shape, Note, Free text and Image are turned off.',
      );
      fireEvent.click(button);
    }
    expect(onTool).not.toHaveBeenCalled();
  });

  it('Image opens a PNG/JPEG picker and hands over the picked file; it is pressed while a stamp is armed', () => {
    const { getByRole, getByLabelText, onImage, rerender } = toolbar();
    const input = getByLabelText('Stamp image') as HTMLInputElement;
    expect(input.type).toBe('file');
    expect(input.accept).toBe('image/png,image/jpeg,.png,.jpg,.jpeg');
    const click = vi.spyOn(input, 'click');
    fireEvent.click(getByRole('button', { name: 'Image' }));
    expect(click).toHaveBeenCalledTimes(1);

    const file = png();
    fireEvent.change(input, { target: { files: [file] } });
    expect(onImage).toHaveBeenCalledWith(file);
    expect(getByRole('button', { name: 'Image' }).getAttribute('aria-pressed')).toBe('false');

    rerender(
      <DrawToolbar tool="stamp" onTool={vi.fn()} color={PALETTE[0]!.color} onColor={vi.fn()} onImage={onImage} allowed ready />,
    );
    expect(getByRole('button', { name: 'Image' }).getAttribute('aria-pressed')).toBe('true');
  });

  it('keeps Image off, described, and its picker shut while annotating is not possible', () => {
    for (const props of [{ allowed: false }, { ready: false }]) {
      const { getByRole, getByLabelText, unmount } = toolbar(props);
      const button = getByRole('button', { name: 'Image' });
      expect(button.getAttribute('aria-disabled')).toBe('true');
      if (props.allowed === false) expect(button.getAttribute('aria-describedby')).toBeTruthy();
      const input = getByLabelText('Stamp image') as HTMLInputElement;
      expect(input.disabled).toBe(true);
      const click = vi.spyOn(input, 'click');
      fireEvent.click(button);
      expect(click).not.toHaveBeenCalled();
      unmount();
    }
  });
});
