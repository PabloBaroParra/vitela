// What Protect says, as pure functions. The dialog's wording is the other
// shells' (Android's strings, which are the Windows shell's); the status
// line is the web's own, because the web does not reopen the protected file
// in place: it hands over a protected copy and the open document stays as it
// was.

import type { SaveOutcome } from '../save/outcome';

/** The name the protected copy is delivered under: "report.pdf" -> "report-protected.pdf", as Android names it. */
export function protectedName(name: string): string {
  const stem = name.trim().replace(/\.pdf$/i, '').trim();
  return `${stem || 'document'}-protected.pdf`;
}

/** The status line for a finished protection. */
export function protectStatus(outcome: SaveOutcome): string {
  switch (outcome.how) {
    case 'cancelled':
      return 'Protection cancelled. No file was written.';
    case 'delivered':
    case 'wrote':
      return `Saved ${outcome.name}, protected with a password. The open document is not protected.`;
    default:
      return `Downloaded ${outcome.name}, protected with a password. The open document is not protected.`;
  }
}
