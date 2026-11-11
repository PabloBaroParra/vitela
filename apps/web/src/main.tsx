import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app/App';
import { startEngine } from './app/engine';
import './app/styles.css';

const engine = startEngine();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App engine={engine} />
  </StrictMode>,
);
