import { afterEach, describe, expect, it, vi } from 'vitest';
import { downloadBytes } from './download';

describe('downloadBytes', () => {
  afterEach(() => vi.useRealTimers());

  it('clicks a temporary link to a PDF blob named after the document, then cleans up', () => {
    vi.useFakeTimers();
    const created: Blob[] = [];
    const revoked: string[] = [];
    vi.stubGlobal('URL', {
      createObjectURL: (blob: Blob) => (created.push(blob), 'blob:vitela/1'),
      revokeObjectURL: (url: string) => revoked.push(url),
    });
    const clicked: { href: string; download: string; attached: boolean }[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      clicked.push({ href: this.href, download: this.download, attached: this.isConnected });
    });

    downloadBytes(new Uint8Array([37, 80, 68, 70]), 'report.pdf');

    expect(created).toHaveLength(1);
    expect(created[0]!.type).toBe('application/pdf');
    expect(created[0]!.size).toBe(4);
    expect(clicked).toEqual([{ href: 'blob:vitela/1', download: 'report.pdf', attached: true }]);
    expect(document.querySelector('a[download]')).toBeNull();
    // The browser needs the URL for a moment after the click.
    expect(revoked).toEqual([]);
    vi.runAllTimers();
    expect(revoked).toEqual(['blob:vitela/1']);
  });

  it('labels the blob with the media type it is given', () => {
    const created: Blob[] = [];
    vi.stubGlobal('URL', { createObjectURL: (blob: Blob) => (created.push(blob), 'blob:vitela/3'), revokeObjectURL: () => {} });
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    downloadBytes(new Uint8Array([0x89, 0x50]), 'report-1.png', 'image/png');
    expect(created[0]!.type).toBe('image/png');
  });
});
