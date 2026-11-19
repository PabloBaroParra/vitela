import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { Annotation, PageSize } from '@vitela/core';
import { AnnotationLayer } from './AnnotationLayer';

const LETTER: PageSize = { widthPt: 612, heightPt: 792, rotationDegrees: 0 };
const yellow = { r: 255, g: 220, b: 0 };
const red = { r: 200, g: 10, b: 10 };

afterEach(cleanup);

const marks = (container: HTMLElement) => [...container.querySelectorAll<SVGElement>('[data-annotation-id]')];

describe('AnnotationLayer', () => {
  it('draws in the page’s own points, so the zoom is only the SVG’s size', () => {
    const { container } = render(<AnnotationLayer page={LETTER} annotations={[]} />);
    const svg = container.querySelector('svg.annotation-layer')!;
    expect(svg.getAttribute('viewBox')).toBe('0 0 612 792');
    expect(svg.getAttribute('aria-hidden')).toBe('true');
  });

  it('draws every kind the snapshot carries', () => {
    const annotations: Annotation[] = [
      { id: 1, pageIndex: 0, kind: 'highlight', rect: { left: 72, top: 100, width: 200, height: 18 }, color: yellow },
      { id: 2, pageIndex: 0, kind: 'underline', rect: { left: 72, top: 130, width: 200, height: 18 }, color: red },
      { id: 3, pageIndex: 0, kind: 'strikeout', rect: { left: 72, top: 160, width: 200, height: 18 }, color: red },
      { id: 4, pageIndex: 0, kind: 'shape', rect: { left: 300, top: 300, width: 50, height: 40 }, color: red },
      { id: 5, pageIndex: 0, kind: 'ink', points: [{ x: 10, y: 10 }, { x: 20, y: 30 }, { x: 40, y: 15 }], color: red },
      { id: 6, pageIndex: 0, kind: 'note', rect: { left: 500, top: 50, width: 24, height: 24 }, contents: 'Look here' },
      { id: 7, pageIndex: 0, kind: 'stamp', rect: { left: 100, top: 500, width: 144, height: 72 } },
      {
        id: 8,
        pageIndex: 0,
        kind: 'freeText',
        rect: { left: 100, top: 600, width: 200, height: 40 },
        contents: 'Hello\nworld',
        layout: {
          fontSizePt: 12,
          overflow: false,
          lines: [
            { text: 'Hello', xPt: 2, baselineFromTopPt: 14 },
            { text: 'world', xPt: 2, baselineFromTopPt: 28 },
          ],
        },
      },
    ];
    const { container } = render(<AnnotationLayer page={LETTER} annotations={annotations} />);
    expect(marks(container).map((mark) => [mark.dataset.annotationId, mark.dataset.kind])).toEqual([
      ['1', 'highlight'],
      ['2', 'underline'],
      ['3', 'strikeout'],
      ['4', 'shape'],
      ['5', 'ink'],
      ['6', 'note'],
      ['7', 'stamp'],
      ['8', 'freeText'],
    ]);

    const band = container.querySelector('[data-annotation-id="1"]')!;
    expect(band.tagName).toBe('rect');
    expect([band.getAttribute('x'), band.getAttribute('y'), band.getAttribute('width'), band.getAttribute('height')]).toEqual([
      '72',
      '100',
      '200',
      '18',
    ]);
    expect(band.getAttribute('fill')).toBe('rgb(255 220 0)');

    const underline = container.querySelector('[data-annotation-id="2"]')!;
    expect(underline.tagName).toBe('line');
    expect(underline.getAttribute('stroke')).toBe('rgb(200 10 10)');
    expect(Number(underline.getAttribute('y1'))).toBeCloseTo(148 - 0.75);

    expect(container.querySelector('[data-annotation-id="5"]')!.getAttribute('points')).toBe('10,10 20,30 40,15');
    expect(container.querySelector('[data-annotation-id="6"] title')!.textContent).toBe('Look here');

    const text = [...container.querySelectorAll('[data-annotation-id="8"] text')];
    expect(text.map((line) => [line.textContent, line.getAttribute('x'), line.getAttribute('y')])).toEqual([
      ['Hello', '2', '14'],
      ['world', '2', '28'],
    ]);
    expect(text[0]!.getAttribute('font-size')).toBe('12');
  });

  it('turns free text with a page drawn turned', () => {
    const turned: PageSize = { widthPt: 792, heightPt: 612, rotationDegrees: 90 };
    const annotation: Annotation = {
      id: 1,
      pageIndex: 0,
      kind: 'freeText',
      rect: { left: 100, top: 200, width: 20, height: 60 },
      contents: 'Hi',
      layout: { fontSizePt: 12, overflow: false, lines: [{ text: 'Hi', xPt: 0, baselineFromTopPt: 12 }] },
    };
    const { container } = render(<AnnotationLayer page={turned} annotations={[annotation]} />);
    expect(container.querySelector('[data-annotation-id="1"]')!.getAttribute('transform')).toBe(
      'translate(110 230) rotate(90) translate(-30 -10)',
    );
  });

  it('draws a stamp with the image kept for its id, else the placeholder box', () => {
    const stamps: Annotation[] = [
      { id: 3, pageIndex: 0, kind: 'stamp', rect: { left: 100, top: 500, width: 144, height: 72 } },
      { id: 4, pageIndex: 0, kind: 'stamp', rect: { left: 300, top: 500, width: 72, height: 144 } },
    ];
    const images = new Map([[3, 'data:image/png;base64,AAAA']]);
    const { container } = render(<AnnotationLayer page={LETTER} annotations={stamps} stampImages={images} />);

    const pictured = container.querySelector('[data-annotation-id="3"]')!;
    expect(pictured.tagName).toBe('g');
    expect(pictured.getAttribute('transform')).toBe('translate(172 536) rotate(0) translate(-72 -36)');
    const image = pictured.querySelector('image')!;
    expect(image.getAttribute('href')).toBe('data:image/png;base64,AAAA');
    expect([image.getAttribute('width'), image.getAttribute('height')]).toEqual(['144', '72']);
    // The core already gave the box the image's proportions: fill it exactly.
    expect(image.getAttribute('preserveAspectRatio')).toBe('none');

    const placeholder = container.querySelector('[data-annotation-id="4"]')!;
    expect(placeholder.tagName).toBe('rect');
    expect(placeholder.getAttribute('class')).toContain('annotation-mark--stamp');
  });

  it('turns a stamp’s image with a page drawn turned, as the saved appearance is', () => {
    const turned: PageSize = { widthPt: 792, heightPt: 612, rotationDegrees: 90 };
    // A 2:1 image on a quarter-turned page: drawn 72 wide, 144 high.
    const stamp: Annotation = { id: 1, pageIndex: 0, kind: 'stamp', rect: { left: 100, top: 200, width: 72, height: 144 } };
    const { container } = render(
      <AnnotationLayer page={turned} annotations={[stamp]} stampImages={new Map([[1, 'data:image/png;base64,AAAA']])} />,
    );
    const mark = container.querySelector('[data-annotation-id="1"]')!;
    expect(mark.getAttribute('transform')).toBe('translate(136 272) rotate(90) translate(-72 -36)');
    const image = mark.querySelector('image')!;
    expect([image.getAttribute('width'), image.getAttribute('height')]).toEqual(['144', '72']);
  });
});
