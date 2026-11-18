// Application shell: engine lifecycle, the route, and top-level layout.

import { useMemo, type ReactNode } from 'react';
import type { EngineClient } from '@vitela/core';
import { DocumentWorkspace } from '../features/document/DocumentWorkspace';
import { HomePage } from '../features/tools/HomePage';
import { resolveRoute, type Route } from '../features/tools/route';
import { ToolPage } from '../features/tools/ToolPage';
import { ToolUnavailable } from '../features/tools/ToolUnavailable';
import { SiteHeader } from '../ui/SiteHeader';
import { StatusLine } from '../ui/StatusLine';
import { useEngineStatus, type EngineStatus } from './useEngineStatus';

interface AppProps {
  readonly engine: EngineClient;
  /** Defaults to the page's own path; each tool opens in its own tab. */
  readonly pathname?: string;
}

/** What the page shows while no document is open, around its open action. */
function introFor(route: Route): (action: ReactNode) => ReactNode {
  if (route.kind === 'tool') {
    const { tool } = route;
    return (action) => <ToolPage tool={tool} action={action} />;
  }
  // Unknown paths get the home page; the host only rewrites tool paths.
  return (action) => <HomePage action={action} />;
}

/** What takes the open action's place until the engine can open documents. */
function standInFor(status: EngineStatus): ReactNode | undefined {
  switch (status.kind) {
    case 'loading':
      return <StatusLine>Loading the PDF engine…</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{`The PDF engine failed to load: ${status.message}`}</StatusLine>;
    case 'ready':
      return undefined;
  }
}

export function App({ engine, pathname = window.location.pathname }: AppProps) {
  const status = useEngineStatus(engine);
  const route = useMemo(() => resolveRoute(pathname), [pathname]);
  const intro = useMemo(() => introFor(route), [route]);
  const appOnly = route.kind === 'tool' && !route.tool.web ? route.tool : null;

  return (
    // data-engine lets E2E tests tell "initial load finished" apart from
    // anything the app does afterwards (see e2e/zero-network.spec.ts).
    <main className="app" data-engine={status.kind}>
      <SiteHeader />
      {appOnly ? (
        <div className="workspace">
          <ToolUnavailable tool={appOnly} />
        </div>
      ) : (
        <DocumentWorkspace
          engine={engine}
          intro={intro}
          initialScreen={route.kind === 'tool' ? route.tool.screen : undefined}
          standIn={standInFor(status)}
        />
      )}
    </main>
  );
}
