// The frame of every intro page (home, tool, coming soon): the scrolling
// area that holds the page's sections, with the footer at the end.

import type { ReactNode } from 'react';
import { SiteFooter } from './SiteFooter';

interface LandingProps {
  readonly children: ReactNode;
}

export function Landing({ children }: LandingProps) {
  return (
    <div className="landing">
      {children}
      <SiteFooter />
    </div>
  );
}
