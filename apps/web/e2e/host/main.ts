/// <reference types="vite/client" />
// E2E fixture: the editor embedded in a host page with styles of its own
// (e2e/embed.spec.ts). Plain createElement: e2e/ is type-checked without JSX.

import { createElement, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import { createEngine } from '@vitela/core';
import { DocumentWorkspace, StatusLine, useEngineStatus } from '@vitela/viewer';
import '@vitela/viewer/style.css';
import './host.css';

const engine = createEngine();

function Editor() {
  const status = useEngineStatus(engine);
  return createElement(DocumentWorkspace, {
    engine,
    intro: (action: ReactNode) => action,
    standIn: status.kind === 'ready' ? undefined : createElement(StatusLine, { children: 'Loading the PDF engine…' }),
  });
}

createRoot(document.getElementById('host-frame')!).render(createElement(Editor));
