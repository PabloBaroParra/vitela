import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app/App';
import { createEngine } from '@vitela/core';
import '@vitela/viewer/style.css';
import './app/styles/index.css';

const engine = createEngine();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App engine={engine} />
  </StrictMode>,
);
