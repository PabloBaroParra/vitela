// Application shell: engine lifecycle and top-level layout.

import type { EngineClient } from '../core/client';
import { DocumentWorkspace } from '../features/document/DocumentWorkspace';
import { StatusLine } from '../ui/StatusLine';
import { useEngineStatus } from './useEngineStatus';

interface AppProps {
  readonly engine: EngineClient;
}

export function App({ engine }: AppProps) {
  const status = useEngineStatus(engine);
  return (
    // data-engine lets E2E tests tell "initial load finished" apart from
    // anything the app does afterwards (see e2e/zero-network.spec.ts).
    <main className="app" data-engine={status.kind}>
      <h1 className="app__title">Vitela</h1>
      {status.kind === 'loading' && <StatusLine>Loading the PDF engine…</StatusLine>}
      {status.kind === 'failed' && (
        <StatusLine tone="error">{`The PDF engine failed to load: ${status.message}`}</StatusLine>
      )}
      {status.kind === 'ready' && <DocumentWorkspace engine={engine} />}
    </main>
  );
}
