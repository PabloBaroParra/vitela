import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { ToolGrid } from './ToolGrid';
import { TOOLS } from './tools';

afterEach(cleanup);

const cardNames = () =>
  within(screen.getByRole('list', { name: 'Tools' }))
    .getAllByRole('heading')
    .map((heading) => heading.textContent);

describe('ToolGrid', () => {
  it('shows every tool as a link to its path in a new tab', () => {
    render(<ToolGrid />);
    expect(cardNames()).toEqual(TOOLS.map((tool) => tool.name));
    const links = within(screen.getByRole('list', { name: 'Tools' })).getAllByRole('link');
    expect(links.map((link) => link.getAttribute('href'))).toEqual(TOOLS.map((tool) => tool.path));
    for (const link of links) {
      expect(link.getAttribute('target')).toBe('_blank');
      expect(link.getAttribute('rel')).toBe('noopener');
    }
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(document.querySelector('input')).toBeNull();
  });

  it('narrows the list to a category, and All restores it', () => {
    render(<ToolGrid />);
    fireEvent.click(screen.getByRole('button', { name: 'Security' }));
    expect(screen.getByRole('button', { name: 'Security' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'All' }).getAttribute('aria-pressed')).toBe('false');
    expect(cardNames()).toEqual(TOOLS.filter((tool) => tool.category === 'security').map((tool) => tool.name));

    fireEvent.click(screen.getByRole('button', { name: 'All' }));
    expect(cardNames()).toHaveLength(TOOLS.length);
  });

  it('only offers categories that have tools', () => {
    render(<ToolGrid tools={TOOLS.filter((tool) => tool.category === 'edit')} />);
    const chips = within(screen.getByRole('group', { name: 'Filter tools by category' })).getAllByRole('button');
    expect(chips.map((chip) => chip.textContent)).toEqual(['All', 'Edit']);
  });

  it('marks tools that are not on the web yet', () => {
    render(<ToolGrid />);
    expect(screen.getAllByText('Coming soon')).toHaveLength(TOOLS.filter((tool) => !tool.web).length);
    expect(screen.getAllByText('Available')).toHaveLength(TOOLS.filter((tool) => tool.web).length);
  });
});
