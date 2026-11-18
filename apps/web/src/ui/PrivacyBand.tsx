// The privacy statement: a muted full-width band with three plain facts.

import type { CSSProperties, Ref } from 'react';

const FACTS: readonly { readonly title: string; readonly body: string; readonly icon: readonly string[] }[] = [
  {
    title: 'Runs in your browser',
    body: 'The PDF engine is compiled to WebAssembly and works on your machine.',
    icon: ['M5.5 4.5h13a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2Z', 'M8.5 20.5h7M12 16.5v4'],
  },
  {
    title: 'No uploads, no account',
    body: 'Nothing is sent to a server, and there is nothing to sign up for.',
    icon: ['M12 3.5l8.3 2.9v5.8c0 4.6-3.5 7.4-8.3 8.3-4.8-.9-8.3-3.7-8.3-8.3V6.4Z', 'M8.9 12.1 11 14.2l4.1-4.2'],
  },
  {
    title: 'Open source',
    body: 'Licensed MIT or Apache-2.0. Anyone can read the code.',
    icon: ['m8.5 7.5-5 4.5 5 4.5', 'm15.5 7.5 5 4.5-5 4.5', 'm13.5 4.5-3 15'],
  },
];

interface PrivacyBandProps {
  /** Lets a container attach a scroll reveal. */
  readonly revealRef?: Ref<HTMLElement>;
}

export function PrivacyBand({ revealRef }: PrivacyBandProps) {
  return (
    <section ref={revealRef} className="privacy-band reveal" aria-labelledby="privacy-title">
      <div className="privacy-band__inner">
        <h2 id="privacy-title" className="privacy-band__title">
          Your PDFs never leave your device
        </h2>
        <ul className="privacy-band__facts">
          {FACTS.map((fact, index) => (
            <li key={fact.title} className="privacy-fact" style={{ '--i': index } as CSSProperties}>
              <svg className="privacy-fact__icon" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
                {fact.icon.map((d) => (
                  <path key={d} d={d} />
                ))}
              </svg>
              <div>
                <h3 className="privacy-fact__title">{fact.title}</h3>
                <p className="privacy-fact__body">{fact.body}</p>
              </div>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
