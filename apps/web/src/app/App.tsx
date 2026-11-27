// Application shell: engine lifecycle, the route, and top-level layout.

import { useMemo, useSyncExternalStore, type ReactNode } from 'react';
import type { EngineClient } from '@vitela/core';
import { DocumentWorkspace, StatusLine, useEngineStatus, type EngineStatus } from '@vitela/viewer';
import { HomePage } from '../features/tools/HomePage';
import { resolveRoute, type Route } from '../features/tools/route';
import { ToolPage } from '../features/tools/ToolPage';
import { ToolUnavailable } from '../features/tools/ToolUnavailable';
import { UpdateBanner } from '../features/pwa/UpdateBanner';
import { NO_UPDATES, type AppUpdates } from '../features/pwa/updates';
import { SiteHeader } from '../ui/SiteHeader';

interface AppProps {
  readonly engine: EngineClient;
  /** Defaults to the page's own path; each tool opens in its own tab. */
  readonly pathname?: string;
  /** New builds of the app (the service worker, production only). Default: none. */
  readonly updates?: AppUpdates;
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

export function App({ engine, pathname = window.location.pathname, updates = NO_UPDATES }: AppProps) {
  const status = useEngineStatus(engine);
  const update = useSyncExternalStore(updates.subscribe, updates.state);
  const route = useMemo(() => resolveRoute(pathname), [pathname]);
  const intro = useMemo(() => introFor(route), [route]);
  const appOnly = route.kind === 'tool' && !route.tool.web ? route.tool : null;

  return (
    // data-engine lets E2E tests tell "initial load finished" apart from
    // anything the app does afterwards (see e2e/zero-network.spec.ts).
    <main className="app" data-engine={status.kind}>
      <SiteHeader />
      {update.ready && <UpdateBanner held={update.held} blocked={update.blocked} onReload={updates.apply} />}
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
          // The page is all editor: every key and dropped PDF on it is the editor's.
          eventScope="window"
          // A new build's reload waits for the edits to be saved.
          onDirtyChange={updates.hold}
        />
      )}
    </main>
  );
}
