import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { ContentImage, ContentRun, PageContent } from '@vitela/core';
import { ContentLayer, type ContentInput } from './ContentLayer';

afterEach(cleanup);

const hello: ContentRun = {
  kind: 'run',
  id: 1,
  pageIndex: 0,
  rect: { left: 100, top: 100, width: 40, height: 12 },
  fontName: 'F1',
  fontKind: 'standard14',
  text: 'Hello',
  editable: true,
  pending: false,
};
const photo: ContentImage = { kind: 'image', id: 3, pageIndex: 0, rect: { left: 200, top: 100, width: 100, height: 50 }, inline: false, pending: false };
const content: PageContent = { runs: [hello], images: [photo], paths: [] };

function setup(props: Partial<Parameters<typeof ContentLayer>[0]> = {}) {
  const calls: unknown[][] = [];
  const input: ContentInput = {
    onHover: () => {},
    onSelect: (...args) => void calls.push(['select', ...args]),
    onToggle: (...args) => void calls.push(['toggle', ...args]),
    onGather: (...args) => void calls.push(['gather', ...args]),
    onOpen: () => {},
    onDrag: (...args) => void calls.push(['drag', ...args]),
  };
  const view = render(
    <ContentLayer pageIndex={0} scale={1} state={{ status: 'ready', content }} hovered={null} selected={null} input={input} {...props} />,
  );
  const layer = view.container.querySelector('.content-layer')!;
  const press = (x: number, y: number, init: object = {}) =>
    fireEvent.pointerDown(layer, { button: 0, isPrimary: true, pointerId: 1, clientX: x, clientY: y, ...init });
  const move = (x: number, y: number) => fireEvent.pointerMove(layer, { pointerId: 1, clientX: x, clientY: y });
  const release = (x: number, y: number) => fireEvent.pointerUp(layer, { pointerId: 1, clientX: x, clientY: y });
  return { calls, layer, press, move, release, view };
}

describe('ContentLayer', () => {
  it('a drag from where nothing is draws a marquee and gathers what it spans', () => {
    const { calls, layer, press, move, release } = setup();
    press(50, 50);
    move(150, 90);
    move(250, 130);
    expect(layer.querySelector('.content-marquee')).not.toBeNull();
    release(250, 130);
    expect(layer.querySelector('.content-marquee')).toBeNull();
    // The press lets the selection go first, as a click beside it does.
    expect(calls).toEqual([
      ['select', 0, { x: 50, y: 50 }],
      ['gather', 0, { left: 50, top: 50, width: 200, height: 80 }, false],
    ]);
  });

  it('a marquee with Shift adds to the selection; a press without a drag gathers nothing', () => {
    const { calls, press, move, release } = setup();
    press(50, 50, { shiftKey: true });
    move(250, 130);
    release(250, 130);
    expect(calls).toEqual([['gather', 0, { left: 50, top: 50, width: 200, height: 80 }, true]]);
    calls.length = 0;
    press(50, 50);
    release(51, 50);
    expect(calls).toEqual([['select', 0, { x: 50, y: 50 }]]);
  });

  it('Shift or Ctrl with a press on an item toggles it', () => {
    const { calls, press } = setup();
    press(110, 105, { shiftKey: true });
    press(210, 110, { ctrlKey: true });
    expect(calls).toEqual([
      ['toggle', 0, { x: 110, y: 105 }],
      ['toggle', 0, { x: 210, y: 110 }],
    ]);
  });

  it('outlines every member of a group inside its box, and drags the group by any point of that box', () => {
    const { calls, layer, press, move, release } = setup({ group: [hello, photo] });
    expect(layer.querySelectorAll('.content-outline[data-state="selected"]')).toHaveLength(2);
    const box = layer.querySelector<HTMLElement>('.content-group')!;
    expect([box.style.left, box.style.top, box.style.width, box.style.height]).toEqual(['100px', '100px', '200px', '50px']);
    // Between the two members, inside the box.
    press(170, 120);
    move(200, 130);
    expect(layer.getAttribute('data-dragging')).toBe('true');
    release(200, 130);
    expect(calls).toEqual([['drag', 0, 30, 10]]);
  });
});
