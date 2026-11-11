// Container: the open-a-PDF flow plus whatever shows the open document.

import type { Engine } from '../../core/types';
import { FilePicker } from '../../ui/FilePicker';
import { StatusLine } from '../../ui/StatusLine';
import { PageView } from '../render/PageView';
import { useDocument } from './useDocument';

interface DocumentWorkspaceProps {
  readonly engine: Engine;
}

export function DocumentWorkspace({ engine }: DocumentWorkspaceProps) {
  const { state, open } = useDocument(engine);
  return (
    <>
      <FilePicker
        label="Open PDF"
        accept="application/pdf,.pdf"
        onPick={open}
        disabled={state.status === 'opening'}
      />
      {state.status === 'opening' && <StatusLine>{`Opening ${state.name}…`}</StatusLine>}
      {state.status === 'failed' && (
        <StatusLine tone="error">{`Could not open ${state.name}: ${state.message}`}</StatusLine>
      )}
      {state.status === 'open' && (
        <>
          <StatusLine>{`${state.name} — ${state.pageCount} ${state.pageCount === 1 ? 'page' : 'pages'}`}</StatusLine>
          <PageView key={state.generation} engine={engine} generation={state.generation} name={state.name} />
        </>
      )}
    </>
  );
}
