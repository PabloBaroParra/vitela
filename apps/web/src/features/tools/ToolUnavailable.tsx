// A tool that is only in the apps for now. It says so plainly and points to
// what the web can do; there is no picker here, since nothing could be done
// with the file.

import { Landing } from '../../ui/Landing';
import { ToolHero } from '../../ui/ToolHero';
import { VIEWER_PATH, type Tool } from './tools';

interface ToolUnavailableProps {
  readonly tool: Tool;
}

export function ToolUnavailable({ tool }: ToolUnavailableProps) {
  return (
    <Landing>
      <ToolHero
        name={tool.name}
        description={tool.description}
        icon={tool.icon}
        tone={tool.tone}
        badge={<span className="pill">Coming to the web</span>}
      >
        <p className="tool-hero__note">
          {`${tool.name} is already in the Vitela desktop and mobile apps, and it is on its way to the web.`}
        </p>
        <div className="tool-hero__links">
          <a className="button-link button-link--primary" href={VIEWER_PATH}>
            Open the PDF viewer
          </a>
          <a className="button-link" href="/">
            See all tools
          </a>
        </div>
      </ToolHero>
    </Landing>
  );
}
