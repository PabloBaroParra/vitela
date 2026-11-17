// The bottom line of every intro page: licence and source link.

import { SOURCE_URL } from './SiteHeader';

export function SiteFooter() {
  return (
    <footer className="site-footer">
      <span>Vitela is open source, licensed MIT or Apache-2.0.</span>
      <a href={SOURCE_URL} target="_blank" rel="noreferrer">
        View on GitHub
      </a>
    </footer>
  );
}
