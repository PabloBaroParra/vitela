import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { MenuButton, type MenuTool } from './MenuButton';

afterEach(cleanup);

function setup(disabled: readonly string[] = []) {
  const chosen: string[] = [];
  const tools: MenuTool[] = ['Compress', 'Export images', 'Protect'].map((label) => ({
    label,
    disabled: disabled.includes(label),
    onSelect: () => chosen.push(label),
  }));
  render(
    <>
      <MenuButton label="More tools" tools={tools} />
      <button type="button">Outside</button>
    </>,
  );
  const button = screen.getByRole('button', { name: 'More tools' });
  return { chosen, button };
}

const items = () => screen.getAllByRole('menuitem');
const focused = () => document.activeElement?.textContent;

describe('MenuButton', () => {
  it('is a closed menu button until pressed', () => {
    const { button } = setup();
    expect(button.getAttribute('aria-haspopup')).toBe('menu');
    expect(button.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('opens on a click with the first item focused, the menu named by and tied to its button', () => {
    const { button } = setup();
    fireEvent.click(button);
    const menu = screen.getByRole('menu', { name: 'More tools' });
    expect(button.getAttribute('aria-expanded')).toBe('true');
    expect(button.getAttribute('aria-controls')).toBe(menu.id);
    expect(items().map((item) => item.textContent)).toEqual(['Compress', 'Export images', 'Protect']);
    expect(focused()).toBe('Compress');
    // One tab stop: the items are reached with the arrows.
    expect(items().map((item) => item.tabIndex)).toEqual([-1, -1, -1]);
  });

  it('opens from the keyboard: Enter, Space and Down on the first item, Up on the last', () => {
    const { button } = setup();
    for (const [key, first] of [
      ['Enter', 'Compress'],
      [' ', 'Compress'],
      ['ArrowDown', 'Compress'],
      ['ArrowUp', 'Protect'],
    ] as const) {
      fireEvent.keyDown(button, { key });
      expect(focused()).toBe(first);
      fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
      expect(screen.queryByRole('menu')).toBeNull();
      expect(document.activeElement).toBe(button);
    }
  });

  it('moves with the arrows, wrapping, and jumps with Home and End', () => {
    const { button } = setup();
    fireEvent.click(button);
    const press = (key: string) => fireEvent.keyDown(document.activeElement!, { key });
    press('ArrowDown');
    expect(focused()).toBe('Export images');
    press('ArrowDown');
    press('ArrowDown');
    expect(focused()).toBe('Compress');
    press('ArrowUp');
    expect(focused()).toBe('Protect');
    press('Home');
    expect(focused()).toBe('Compress');
    press('End');
    expect(focused()).toBe('Protect');
  });

  it('runs the item chosen with Enter or a click, closing first and giving the focus back to the button', () => {
    const { button, chosen } = setup();
    fireEvent.click(button);
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowDown' });
    fireEvent.keyDown(document.activeElement!, { key: 'Enter' });
    expect(chosen).toEqual(['Export images']);
    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(button);

    fireEvent.click(button);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Protect' }));
    expect(chosen).toEqual(['Export images', 'Protect']);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('keeps a busy tool in the list but does not run it', () => {
    const { button, chosen } = setup(['Compress']);
    fireEvent.click(button);
    const compress = screen.getByRole('menuitem', { name: 'Compress' });
    expect(compress.getAttribute('aria-disabled')).toBe('true');
    fireEvent.keyDown(compress, { key: 'Enter' });
    fireEvent.click(compress);
    expect(chosen).toEqual([]);
    expect(screen.getByRole('menu')).toBeTruthy();
  });

  it('closes on Tab, on a press outside it and on a second press of its button', () => {
    const { button } = setup();
    fireEvent.click(button);
    fireEvent.keyDown(document.activeElement!, { key: 'Tab' });
    expect(screen.queryByRole('menu')).toBeNull();

    fireEvent.click(button);
    fireEvent.pointerDown(screen.getByRole('button', { name: 'Outside' }));
    expect(screen.queryByRole('menu')).toBeNull();

    fireEvent.click(button);
    fireEvent.click(button);
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('does not let its keys reach the editor behind it', () => {
    const { button } = setup();
    let reached = 0;
    const count = () => reached++;
    document.addEventListener('keydown', count);
    fireEvent.click(button);
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    document.removeEventListener('keydown', count);
    expect(reached).toBe(0);
  });
  it('stays shut while disabled, focusable and said to be off', () => {
    const chosen: string[] = [];
    render(<MenuButton label="Add blank page" disabled tools={[{ label: 'Portrait A4', onSelect: () => chosen.push('p') }]} />);
    const button = screen.getByRole('button', { name: 'Add blank page' });
    expect(button.getAttribute('aria-disabled')).toBe('true');
    fireEvent.click(button);
    fireEvent.keyDown(button, { key: 'Enter' });
    fireEvent.keyDown(button, { key: 'ArrowDown' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(button.getAttribute('aria-expanded')).toBe('false');
    expect(chosen).toEqual([]);
  });

  it('closes for good when it turns off while open, the focus back on its button', () => {
    const tools: MenuTool[] = [{ label: 'Portrait A4', onSelect: () => {} }];
    const { rerender } = render(<MenuButton label="Add blank page" tools={tools} />);
    const button = screen.getByRole('button', { name: 'Add blank page' });
    fireEvent.click(button);
    expect(focused()).toBe('Portrait A4');

    rerender(<MenuButton label="Add blank page" disabled tools={tools} />);
    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(button);

    // Turning back on does not reopen what was open before.
    rerender(<MenuButton label="Add blank page" tools={tools} />);
    expect(screen.queryByRole('menu')).toBeNull();
    expect(button.getAttribute('aria-expanded')).toBe('false');
  });

  it('leaves the focus where it is when it turns off while closed', () => {
    const tools: MenuTool[] = [{ label: 'Portrait A4', onSelect: () => {} }];
    const { rerender } = render(
      <>
        <MenuButton label="Add blank page" tools={tools} />
        <button type="button">Outside</button>
      </>,
    );
    const outside = screen.getByRole('button', { name: 'Outside' });
    outside.focus();
    rerender(
      <>
        <MenuButton label="Add blank page" disabled tools={tools} />
        <button type="button">Outside</button>
      </>,
    );
    expect(document.activeElement).toBe(outside);
  });

  it('shows a toggle as a menuitemcheckbox, and reads a refused one with its reason', () => {
    const chosen: string[] = [];
    render(
      <MenuButton
        label="Tools"
        tools={[
          { label: 'Compress', onSelect: () => chosen.push('Compress') },
          { label: 'Edit content', checked: false, disabled: true, description: 'Content editing is not permitted.', onSelect: () => chosen.push('Edit content') },
        ]}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Tools' }));
    expect(screen.getAllByRole('menuitem').map((item) => item.textContent)).toEqual(['Compress']);
    const toggle = screen.getByRole('menuitemcheckbox', { name: 'Edit content' });
    expect(toggle.getAttribute('aria-checked')).toBe('false');
    expect(toggle.getAttribute('aria-disabled')).toBe('true');
    expect(document.getElementById(toggle.getAttribute('aria-describedby') ?? '')?.textContent).toBe('Content editing is not permitted.');
    fireEvent.click(toggle);
    expect(chosen).toEqual([]);
    expect(screen.getByRole('menu')).toBeTruthy();
  });

  it('chooses a toggle like an action, checked or not', () => {
    const chosen: boolean[] = [];
    render(<MenuButton label="Tools" tools={[{ label: 'Edit content', checked: true, onSelect: () => chosen.push(true) }]} />);
    fireEvent.click(screen.getByRole('button', { name: 'Tools' }));
    const toggle = screen.getByRole('menuitemcheckbox', { name: 'Edit content' });
    expect(toggle.getAttribute('aria-checked')).toBe('true');
    expect(toggle.getAttribute('aria-describedby')).toBeNull();
    fireEvent.click(toggle);
    expect(chosen).toEqual([true]);
    expect(screen.queryByRole('menu')).toBeNull();
  });
});
