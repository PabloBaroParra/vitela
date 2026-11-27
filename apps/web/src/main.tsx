import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app/App';
import { createEngine } from '@vitela/core';
import { NO_LAUNCHES, launchQueueOf, watchLaunches } from './features/launch/launchQueue';
import { NO_UPDATES, watchForUpdates, windowLoaded } from './features/pwa/updates';
import { unloadWarning } from './features/unload/unloadWarning';
import '@vitela/viewer/style.css';
import './app/styles/index.css';

const engine = createEngine();

// The service worker (offline use, installable app) is a production build's
// only: the dev server and Vitest never register it, so they always run the
// code on disk. It registers after the window's `load`, so its ~9 MB precache
// never competes with the app's own first load.
const updates =
  import.meta.env.PROD && 'serviceWorker' in navigator
    ? watchForUpdates(navigator.serviceWorker, () => window.location.reload(), windowLoaded(window))
    : NO_UPDATES;

// PDFs the OS opened the installed app with (Chromium only). Taken from the
// start: the browser holds the launch until a consumer is set.
const launchQueue = launchQueueOf(window);
const launches = launchQueue ? watchLaunches(launchQueue) : NO_LAUNCHES;

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App engine={engine} updates={updates} launches={launches} warnOnUnload={unloadWarning(window)} />
  </StrictMode>,
);
