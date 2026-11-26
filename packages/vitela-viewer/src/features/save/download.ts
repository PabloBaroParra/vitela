// Saving by download: the way out where no file handle exists (Firefox,
// WebKit, or a file that arrived without one). The bytes never leave the
// machine — a Blob URL is local to the page.

/** The media type of every file the editor writes, except an exported page image. */
export const PDF_TYPE = 'application/pdf';

/** Hands `bytes` to the browser as a file called `name`, of media type `type` (a PDF unless told). */
export function downloadBytes(bytes: Uint8Array, name: string, type = PDF_TYPE): void {
  const url = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>], { type }));
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
