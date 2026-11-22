import { act, cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { useEditorScope, WHOLE_DOCUMENT, type EventScope, type EditorScope } from './editorScope';

afterEach(() => {
  cleanup();
  document.body.replaceChildren();
});

/** Mounts an editor root (with a button and a plain page area) next to a host button. */
function mount(eventScope: EventScope = 'editor') {
  let scope: EditorScope = WHOLE_DOCUMENT;
  function Editor() {
    const editor = useEditorScope(eventScope);
    scope = editor.scope;
    return (
      <div ref={editor.rootRef} data-testid="root">
        <button type="button">Editor button</button>
        <div data-testid="page">page</div>
      </div>
    );
  }
  const view = render(
    <>
      <button type="button">Host button</button>
      <Editor />
    </>,
  );
  const hostButton = view.getByRole('button', { name: 'Host button' });
  const editorButton = view.getByRole('button', { name: 'Editor button' });
  const pageArea = view.getByTestId('page');
  const press = (target: Element, type = 'pointerdown') =>
    act(() => {
      target.dispatchEvent(new Event(type, { bubbles: true }));
    });
  return { scope: () => scope, hostButton, editorButton, pageArea, root: view.getByTestId('root'), press };
}

describe('useEditorScope', () => {
  it('takes keys aimed inside the editor root, never ones aimed at the host', () => {
    const { scope, hostButton, editorButton } = mount();
    expect(scope().ownsEvent(editorButton)).toBe(true);
    expect(scope().ownsEvent(hostButton)).toBe(false);
  });

  it('takes keys aimed at the body while the last press was inside the editor', () => {
    const { scope, hostButton, pageArea, press } = mount();
    // Nothing pressed yet: a key on the body belongs to the page.
    expect(scope().ownsEvent(document.body)).toBe(false);
    // A click on the page area focuses nothing, so the key goes to the body.
    press(pageArea);
    expect(scope().ownsEvent(document.body)).toBe(true);
    press(hostButton);
    expect(scope().ownsEvent(document.body)).toBe(false);
  });

  it('takes keys aimed at the body after a file was dropped on the editor', () => {
    const { scope, hostButton, pageArea, press } = mount();
    // A drop is no press and moves no focus, yet the user acted in the editor.
    press(pageArea, 'drop');
    expect(scope().ownsEvent(document.body)).toBe(true);
    press(hostButton, 'drop');
    expect(scope().ownsEvent(document.body)).toBe(false);
  });

  it('follows focus moved by the keyboard too', () => {
    const { scope, hostButton, editorButton } = mount();
    act(() => editorButton.focus());
    expect(scope().ownsEvent(document.body)).toBe(true);
    act(() => hostButton.focus());
    expect(scope().ownsEvent(document.body)).toBe(false);
  });

  it('takes only drops over the editor root by default', () => {
    const { scope, hostButton, pageArea } = mount();
    expect(scope().takesDrop(pageArea)).toBe(true);
    expect(scope().takesDrop(hostButton)).toBe(false);
    expect(scope().takesDrop(document.body)).toBe(false);
  });

  it('takes every key and drop on the window when asked to, like a page that is all editor', () => {
    const { scope, hostButton, press } = mount('window');
    expect(scope().takesDrop(hostButton)).toBe(true);
    expect(scope().takesDrop(document.body)).toBe(true);
    // Keys too, wherever focus is and whatever was pressed last.
    press(hostButton);
    expect(scope().ownsEvent(hostButton)).toBe(true);
    expect(scope().ownsEvent(document.body)).toBe(true);
  });

  it('still sees only its own dialogs when taking the whole window', () => {
    const { scope } = mount('window');
    document.body.appendChild(document.createElement('dialog')).setAttribute('open', '');
    expect(scope().modalOpen()).toBe(false);
  });

  it('sees only the modal dialogs inside the editor root', () => {
    const { scope, root } = mount();
    const hostDialog = document.body.appendChild(document.createElement('dialog'));
    hostDialog.setAttribute('open', '');
    expect(scope().modalOpen()).toBe(false);
    const own = root.appendChild(document.createElement('dialog'));
    own.setAttribute('open', '');
    expect(scope().modalOpen()).toBe(true);
  });
});

describe('WHOLE_DOCUMENT', () => {
  it('takes every key and drop and sees every dialog on the page', () => {
    expect(WHOLE_DOCUMENT.ownsEvent(document.body)).toBe(true);
    expect(WHOLE_DOCUMENT.takesDrop(document.body)).toBe(true);
    expect(WHOLE_DOCUMENT.modalOpen()).toBe(false);
    document.body.appendChild(document.createElement('dialog')).setAttribute('open', '');
    expect(WHOLE_DOCUMENT.modalOpen()).toBe(true);
  });
});
