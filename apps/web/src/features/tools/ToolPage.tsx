// A tool this web build can do: its hero with the open action. Once a
// document is open the workspace replaces this with the viewer.

import type { ReactNode } from 'react';
import { Landing } from '../../ui/Landing';
import { PrivacyBand } from '../../ui/PrivacyBand';
import { ToolHero } from '../../ui/ToolHero';
import type { Tool } from './tools';
import { useReveal } from './useReveal';

interface ToolPageProps {
  readonly tool: Tool;
  readonly action: ReactNode;
}

export function ToolPage({ tool, action }: ToolPageProps) {
  const privacyReveal = useReveal<HTMLElement>();
  return (
    <Landing>
      <ToolHero name={tool.name} description={tool.description} icon={tool.icon} tone={tool.tone}>
        <div className="tool-hero__action">{action}</div>
        <p className="hero__drop">or drop a PDF anywhere on this page</p>
        <a className="text-link" href="/">
          See all tools
        </a>
      </ToolHero>
      <PrivacyBand revealRef={privacyReveal} />
    </Landing>
  );
}
