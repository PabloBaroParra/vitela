// Saving by download: the way out where no file handle exists (Firefox,
// WebKit, or a file that arrived without one). The bytes never leave the
// machine — a Blob URL is local to the page.

/** Hands `bytes` to the browser as a file called `name`. */
export function downloadBytes(bytes: Uint8Array, name: string): void {
  const url = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>], { type: 'application/pdf' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.hidden = true;
  // WebKit and Firefox only honour a click on a link that is in the document.
  document.body.append(link);
  link.click();
  link.remove();
  // The download starts asynchronously and still needs the URL.
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
