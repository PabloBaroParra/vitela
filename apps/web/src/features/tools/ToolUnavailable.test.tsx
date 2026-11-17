import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { ToolUnavailable } from './ToolUnavailable';
import { TOOLS } from './tools';

afterEach(cleanup);

const edit = TOOLS.find((tool) => tool.id === 'edit')!;

describe('ToolUnavailable', () => {
  it('names the tool and says it is coming to the web', () => {
    render(<ToolUnavailable tool={edit} />);
    expect(screen.getByRole('heading', { level: 2, name: 'Edit PDF' })).toBeTruthy();
    expect(screen.getByText('Coming to the web')).toBeTruthy();
    expect(screen.getByText(/already in the Vitela desktop and mobile apps/)).toBeTruthy();
  });

  it('offers no file input', () => {
    render(<ToolUnavailable tool={edit} />);
    expect(document.querySelector('input')).toBeNull();
    expect(screen.queryByLabelText('Open PDF')).toBeNull();
  });

  it('links to the viewer and back to all tools', () => {
    render(<ToolUnavailable tool={edit} />);
    expect(screen.getByRole('link', { name: 'Open the PDF viewer' }).getAttribute('href')).toBe('/view');
    expect(screen.getByRole('link', { name: 'See all tools' }).getAttribute('href')).toBe('/');
  });
});
