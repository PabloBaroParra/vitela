// Which file of a drag-and-drop the viewer opens. Pure.

const isPdf = (file: File) => file.type === 'application/pdf' || file.name.toLowerCase().endsWith('.pdf');

/**
 * The first PDF among `files`, else the first file at all — opening that
 * lets the engine say why it is not a PDF instead of the drop doing nothing.
 */
export function droppedFile(files: readonly File[]): File | null {
  return files.find(isPdf) ?? files[0] ?? null;
}

/** Whether a drag carries files (and not, say, selected text or a link). */
export function carriesFiles(types: readonly string[] | undefined): boolean {
  return types?.includes('Files') ?? false;
}
