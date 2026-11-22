// A React host: <VitelaViewer> opens a PDF from a URL, the host receives
// every save in `onSave`, and can hand the saved bytes back to reopen them.
import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { VitelaViewer, type ViewerSource } from '@vitela/viewer';
import '@vitela/viewer/style.css';
import './host.css';

interface Saved {
  readonly bytes: Uint8Array;
  readonly name: string;
}

/** What the host shows about a save: the name, the size and the file's first bytes. */
function describe({ bytes, name }: Saved): string {
  const head = new TextDecoder().decode(bytes.subarray(0, 5));
  return `${name}: ${bytes.byteLength} bytes, starts ${head}`;
}

function Host() {
  const [source, setSource] = useState<ViewerSource>('/sample.pdf');
  const [name, setName] = useState<string | undefined>(undefined);
  const [saved, setSaved] = useState<Saved | null>(null);

  return (
    <>
      <header className="host-header">
        <h1>A React page</h1>
        <output id="saved">{saved ? describe(saved) : 'Nothing saved yet'}</output>
        <button
          type="button"
          disabled={!saved}
          onClick={() => {
            if (!saved) return;
            setSource(saved.bytes);
            setName('saved.pdf');
          }}
        >
          Open the saved copy
        </button>
      </header>
      {/* The editor fills its container: give it a height. */}
      <div className="host-frame">
        <VitelaViewer source={source} name={name} onSave={(bytes, savedName) => setSaved({ bytes, name: savedName })} />
      </div>
    </>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Host />
  </StrictMode>,
);
