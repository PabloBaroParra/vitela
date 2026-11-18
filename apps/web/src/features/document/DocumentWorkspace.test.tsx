import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DocumentInfo, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { DocumentWorkspace } from './DocumentWorkspace';

const info: DocumentInfo = {
  pageCount: 2,
  pages: Array(2).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 }),
  pageIds: [0, 1],
  textExtractionAllowed: true,
  documentAssemblyAllowed: true,
};
const page: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };
const engine = fakeEngine({
  open: async () => ({ handle: 1, info }),
  renderPage: async () => page,
});

beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    putImageData: () => {},
  } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal('ImageData', class {});
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function openPdf(getByLabelText: (label: string) => HTMLElement) {
  const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'two.pdf', { type: 'application/pdf' });
  fireEvent.change(getByLabelText('Open PDF'), { target: { files: [pdf] } });
}

describe('DocumentWorkspace', () => {
  it('opens a document in the viewer by default', async () => {
    const { getByLabelText, getByRole } = render(<DocumentWorkspace engine={engine} intro={(action) => action} />);
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'View' }).getAttribute('aria-pressed')).toBe('true'));
  });

  it('opens a document in the screen its tool asks for', async () => {
    const { getByLabelText, getByRole } = render(
      <DocumentWorkspace engine={engine} intro={(action) => action} initialScreen="organize" />,
    );
    openPdf(getByLabelText);
    await waitFor(() => expect(getByRole('button', { name: 'Organize' }).getAttribute('aria-pressed')).toBe('true'));
  });
});
