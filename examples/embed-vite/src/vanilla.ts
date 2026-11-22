// A host without React of its own: mountViewer() and the handle it returns.
import { mountViewer, type MountedViewer } from '@vitela/viewer';
import '@vitela/viewer/style.css';
import './host.css';

const output = document.getElementById('saved')!;
const saved: { bytes: Uint8Array; name: string }[] = [];

const viewer = mountViewer(document.getElementById('editor')!, {
  filePicker: false,
  onSave: (bytes, name) => {
    saved.push({ bytes, name });
    const head = new TextDecoder().decode(bytes.subarray(0, 5));
    output.textContent = `${name}: ${bytes.byteLength} bytes, starts ${head}`;
  },
});
viewer.open('/sample.pdf');

// For the E2E test (e2e/embed.spec.ts) to drive the handle.
declare global {
  interface Window {
    viewer: MountedViewer;
    saved: typeof saved;
  }
}
window.viewer = viewer;
window.saved = saved;
