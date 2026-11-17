// The top bar: the Vitela mark and name (a link home), plus the source link.

import markLight from '../../../../assets/brand/vitela-app-mark.svg';
import markDark from '../../../../assets/brand/vitela-app-mark-dark.svg';

export const SOURCE_URL = 'https://github.com/PabloBaroParra/vitela';

export function SiteHeader() {
  return (
    <header className="site-header">
      <h1 className="site-header__brand">
        <a href="/">
          <picture>
            <source srcSet={markDark} media="(prefers-color-scheme: dark)" />
            <img src={markLight} alt="" width={28} height={28} />
          </picture>
          Vitela
        </a>
      </h1>
      <nav className="site-header__nav" aria-label="Project">
        <a href={SOURCE_URL} target="_blank" rel="noreferrer">
          Source code
        </a>
      </nav>
    </header>
  );
}
