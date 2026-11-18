// The home page: the open action up front, then every tool, then the
// privacy note. The caller supplies the action (picker or engine status).

import type { ReactNode } from 'react';
import { HomeHero } from '../../ui/HomeHero';
import { Landing } from '../../ui/Landing';
import { PrivacyBand } from '../../ui/PrivacyBand';
import { ToolGrid } from './ToolGrid';
import { useReveal } from './useReveal';

interface HomePageProps {
  readonly action: ReactNode;
}

export function HomePage({ action }: HomePageProps) {
  const toolsReveal = useReveal<HTMLElement>();
  const privacyReveal = useReveal<HTMLElement>();
  return (
    <Landing>
      <HomeHero action={action} />
      <section ref={toolsReveal} className="tools-section reveal" aria-labelledby="tools-title">
        <h2 id="tools-title" className="section-title">
          PDF tools
        </h2>
        <p className="section-lead">
          View PDFs right here. The other tools are in the Vitela apps today and are coming to the web one at a time.
        </p>
        <ToolGrid />
      </section>
      <PrivacyBand revealRef={privacyReveal} />
    </Landing>
  );
}
