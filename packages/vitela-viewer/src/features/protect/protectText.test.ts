import { describe, expect, it } from 'vitest';
import { protectStatus, protectedName } from './protectText';

describe('protectedName', () => {
  it('adds -protected before the extension, as Android names it', () => {
    expect(protectedName('report.pdf')).toBe('report-protected.pdf');
    expect(protectedName('Scan.PDF')).toBe('Scan-protected.pdf');
    expect(protectedName('notes')).toBe('notes-protected.pdf');
  });

  it('falls back to "document" for a name with nothing before the extension', () => {
    expect(protectedName('.pdf')).toBe('document-protected.pdf');
    expect(protectedName('  ')).toBe('document-protected.pdf');
  });
});

describe('protectStatus', () => {
  it('says where the copy went and that the open document is not protected', () => {
    expect(protectStatus({ how: 'downloaded', name: 'a-protected.pdf', fellBack: null })).toBe(
      'Downloaded a-protected.pdf, protected with a password. The open document is not protected.',
    );
    expect(protectStatus({ how: 'delivered', name: 'a-protected.pdf' })).toBe(
      'Saved a-protected.pdf, protected with a password. The open document is not protected.',
    );
  });

  it('says nothing was written when the delivery was cancelled', () => {
    expect(protectStatus({ how: 'cancelled' })).toBe('Protection cancelled. No file was written.');
  });
});
