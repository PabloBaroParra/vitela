// A menu button (WAI-ARIA APG "menu button" pattern): one button that opens
// a list of actions. Generic and presentational: the workspace bar's "Tools"
// menu and Organize's "Add blank page" menu are both this, and
// each item's action is the caller's.
//
// Keyboard: Enter, Space or Down opens on the first item, Up on the last;
// Down/Up move (wrapping), Home/End jump; Enter or Space runs the item;
// Escape closes; Tab closes and moves on. A press outside closes it. Choosing
// an item closes the menu and gives the focus back to the button BEFORE the
// action runs, so a dialog the action opens returns the focus there too.
//
// Turning it off while it is open closes it for good (turning it back on does
// not reopen it), and the focus, when it was in the menu, goes back to the
// button rather than to the page.

import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from 'react';

export interface MenuTool {
  readonly label: string;
  /** Busy: listed, but choosing it does nothing. */
  readonly disabled?: boolean;
  readonly onSelect: () => void;
}

interface MenuButtonProps {
  /** The button's accessible name, and the menu's. */
  readonly label: string;
  readonly tools: readonly MenuTool[];
  readonly className?: string;
  /** What the button shows; the label when not given. */
  readonly icon?: ReactNode;
  /** The menu's button, for a caller that has to give it the focus. */
  readonly buttonRef?: RefObject<HTMLButtonElement | null>;
  /**
   * Off: the button stays shut. `aria-disabled`, not `disabled`, so it keeps
   * the focus when it turns off under it.
   */
  readonly disabled?: boolean;
}

export function MenuButton({ label, tools, className, icon, buttonRef, disabled = false }: MenuButtonProps) {
  /** The item that has the focus while open, or `null` when closed. */
  const [active, setActive] = useState<number | null>(null);
  // Still open on the render that turns it off: the layout effect below
  // closes it before the paint, while the focus can still be seen in it.
  const open = active !== null;
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const items = useRef<(HTMLLIElement | null)[]>([]);
  const menuId = useId();
  const buttonId = useId();

  useEffect(() => {
    if (active !== null) items.current[active]?.focus();
  }, [active]);

  useLayoutEffect(() => {
    if (!disabled || active === null) return;
    const focusInside = root.current?.contains(document.activeElement) ?? false;
    setActive(null);
    if (focusInside) button.current?.focus();
  }, [disabled, active]);

  // A press anywhere else closes it.
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setActive(null);
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [open]);

  const close = useCallback(() => {
    setActive(null);
    button.current?.focus();
  }, []);

  const choose = (tool: MenuTool) => {
    if (tool.disabled) return;
    close();
    tool.onSelect();
  };

  const last = tools.length - 1;

  const onButtonKey = (event: KeyboardEvent) => {
    if (disabled) return;
    const at = { Enter: 0, ' ': 0, ArrowDown: 0, ArrowUp: last }[event.key];
    if (at === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    setActive(at);
  };

  const onMenuKey = (event: KeyboardEvent) => {
    if (active === null) return;
    if (event.key === 'Tab') {
      // Leaves for the next control, as the Tab would anyway.
      setActive(null);
      return;
    }
    const move: Record<string, () => void> = {
      ArrowDown: () => setActive(active === last ? 0 : active + 1),
      ArrowUp: () => setActive(active === 0 ? last : active - 1),
      Home: () => setActive(0),
      End: () => setActive(last),
      Escape: close,
      Enter: () => choose(tools[active]!),
      ' ': () => choose(tools[active]!),
    };
    const act = move[event.key];
    if (!act) return;
    event.preventDefault();
    event.stopPropagation();
    act();
  };

  return (
    <div ref={root} className={`menu-button${className ? ` ${className}` : ''}`}>
      <button
        ref={(element) => {
          button.current = element;
          if (buttonRef) buttonRef.current = element;
        }}
        id={buttonId}
        type="button"
        className="menu-button__button"
        title={label}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-disabled={disabled || undefined}
        onClick={() => (open || disabled ? setActive(null) : setActive(0))}
        onKeyDown={onButtonKey}
        // Space is handled on its keydown; the keyup that follows (also the
        // one of a Space that chose an item and sent the focus back here)
        // must not click the button again.
        onKeyUp={(event) => event.key === ' ' && event.preventDefault()}
      >
        {icon ?? label}
      </button>
      {open && (
        <ul id={menuId} className="menu-button__list" role="menu" aria-labelledby={buttonId} onKeyDown={onMenuKey}>
          {tools.map((tool, index) => (
            <li
              key={tool.label}
              ref={(element) => {
                items.current[index] = element;
              }}
              role="menuitem"
              tabIndex={-1}
              aria-disabled={tool.disabled || undefined}
              className="menu-button__item"
              onClick={() => choose(tool)}
              onPointerMove={() => setActive(index)}
            >
              {tool.label}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
