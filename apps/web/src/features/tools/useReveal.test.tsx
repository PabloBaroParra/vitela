import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PENDING_CLASS, useReveal } from './useReveal';

afterEach(cleanup);

function Probe() {
  return (
    <section ref={useReveal<HTMLElement>()} aria-label="probe">
      content
    </section>
  );
}

describe('useReveal', () => {
  it('never hides anything without IntersectionObserver', () => {
    vi.stubGlobal('IntersectionObserver', undefined);
    render(<Probe />);
    expect(screen.getByLabelText('probe').classList.contains(PENDING_CLASS)).toBe(false);
  });

  it('hides the element until it intersects, then reveals it', () => {
    let notify: (entries: Partial<IntersectionObserverEntry>[]) => void = () => {};
    const disconnect = vi.fn();
    vi.stubGlobal(
      'IntersectionObserver',
      class {
        constructor(callback: (entries: Partial<IntersectionObserverEntry>[]) => void) {
          notify = callback;
        }
        observe() {}
        disconnect = disconnect;
      },
    );
    render(<Probe />);
    const section = screen.getByLabelText('probe');
    expect(section.classList.contains(PENDING_CLASS)).toBe(true);

    act(() => notify([{ isIntersecting: false }]));
    expect(section.classList.contains(PENDING_CLASS)).toBe(true);

    act(() => notify([{ isIntersecting: true }]));
    expect(section.classList.contains(PENDING_CLASS)).toBe(false);
    expect(disconnect).toHaveBeenCalled();
  });
});
