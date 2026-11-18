import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DrawToolbar } from './DrawToolbar';
import { PALETTE, type DrawTool } from './drawing';

afterEach(cleanup);

function toolbar(props: { tool?: DrawTool; allowed?: boolean; ready?: boolean } = {}) {
  const onTool = vi.fn();
  const onColor = vi.fn();
  const view = render(
    <DrawToolbar
      tool={props.tool ?? 'select'}
      onTool={onTool}
      color={PALETTE[0]!.color}
      onColor={onColor}
      allowed={props.allowed ?? true}
      ready={props.ready ?? true}
    />,
  );
  return { ...view, onTool, onColor };
}

const TOOLS = ['Ink', 'Shape', 'Note', 'Free text'];

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
      expect(note?.textContent).toBe('This PDF does not allow annotations, so Ink, Shape, Note and Free text are turned off.');
      fireEvent.click(button);
    }
    expect(onTool).not.toHaveBeenCalled();
  });
});
