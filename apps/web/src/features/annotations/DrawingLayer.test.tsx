import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { PageSize } from '@vitela/core';
import { DrawingLayer } from './DrawingLayer';
import type { ArmedTool } from './drawing';

const LETTER: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };
const YELLOW = { r: 255, g: 220, b: 0 };
/** 150 % zoom: CSS px per point (4/3 × 1.5). */
const SCALE = 2;

afterEach(cleanup);

function layer(tool: ArmedTool, busy = false) {
  const onGesture = vi.fn();
  const { container } = render(
    <DrawingLayer
      pageIndex={3}
      page={LETTER}
      pageKey="9/0"
      scale={SCALE}
      tool={tool}
      color={YELLOW}
      busy={busy}
      onGesture={onGesture}
    />,
  );
  const surface = container.querySelector('.drawing-layer') as HTMLDivElement;
  return { surface, onGesture, container };
}

/** A pointer event at (`x`, `y`) points on the page. */
const at = (x: number, y: number) => ({ pointerId: 1, button: 0, isPrimary: true, clientX: x * SCALE, clientY: y * SCALE });

describe('DrawingLayer', () => {
  it('hands a stroke in drawn points to the viewer with the page it began on, previewing it meanwhile', () => {
    const { surface, onGesture, container } = layer('ink');
    fireEvent.pointerDown(surface, at(10, 20));
    fireEvent.pointerMove(surface, at(30, 40));
    expect(container.querySelector('polyline')?.getAttribute('points')).toBe('10,20 30,40');
    fireEvent.pointerUp(surface, at(50, 40));

    expect(onGesture).toHaveBeenCalledWith(3, '9/0', 'ink', {
      kind: 'stroke',
      points: [
        { x: 10, y: 20 },
        { x: 30, y: 40 },
        { x: 50, y: 40 },
      ],
    });
    expect(container.querySelector('svg')).toBeNull();
  });

  it('traces a shape box, previewed as it is dragged', () => {
    const { surface, onGesture, container } = layer('shape');
    fireEvent.pointerDown(surface, at(100, 100));
    fireEvent.pointerMove(surface, at(160, 140));
    const preview = container.querySelector('rect');
    expect([preview?.getAttribute('width'), preview?.getAttribute('height')]).toEqual(['60', '40']);
    fireEvent.pointerUp(surface, at(180, 150));
    expect(onGesture).toHaveBeenCalledWith(3, '9/0', 'shape', {
      kind: 'box',
      rect: { left: 100, top: 100, width: 80, height: 50 },
    });
  });

  it('a click places a note box', () => {
    const { surface, onGesture } = layer('note');
    fireEvent.pointerDown(surface, at(300, 400));
    fireEvent.pointerUp(surface, at(300, 400));
    expect(onGesture).toHaveBeenCalledWith(3, '9/0', 'note', {
      kind: 'box',
      rect: { left: 300, top: 400, width: 24, height: 24 },
    });
  });

  it('starts nothing while busy, and drops a cancelled gesture', () => {
    const busy = layer('shape', true);
    fireEvent.pointerDown(busy.surface, at(10, 10));
    fireEvent.pointerUp(busy.surface, at(80, 80));
    expect(busy.onGesture).not.toHaveBeenCalled();
    cleanup();

    const { surface, onGesture, container } = layer('ink');
    fireEvent.pointerDown(surface, at(10, 10));
    fireEvent.pointerMove(surface, at(40, 40));
    fireEvent.pointerCancel(surface, at(40, 40));
    expect(onGesture).not.toHaveBeenCalled();
    expect(container.querySelector('svg')).toBeNull();
  });

  it('ignores secondary buttons', () => {
    const { surface, onGesture } = layer('note');
    fireEvent.pointerDown(surface, { ...at(10, 10), button: 2 });
    fireEvent.pointerUp(surface, at(10, 10));
    expect(onGesture).not.toHaveBeenCalled();
  });

  it('drops a gesture whose pointer capture is lost, and takes the next one', () => {
    const { surface, onGesture, container } = layer('ink');
    fireEvent.pointerDown(surface, at(10, 10));
    fireEvent.pointerMove(surface, at(40, 40));
    fireEvent.lostPointerCapture(surface, at(40, 40));
    expect(container.querySelector('svg')).toBeNull();
    fireEvent.pointerUp(surface, at(50, 50));
    expect(onGesture).not.toHaveBeenCalled();

    fireEvent.pointerDown(surface, { ...at(100, 100), pointerId: 2 });
    fireEvent.pointerUp(surface, { ...at(120, 100), pointerId: 2 });
    expect(onGesture).toHaveBeenCalledTimes(1);
    expect(onGesture.mock.calls[0]![3]).toMatchObject({ kind: 'stroke' });
  });
});
