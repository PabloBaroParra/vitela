import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { UpdateBanner } from './UpdateBanner';

afterEach(cleanup);

describe('UpdateBanner', () => {
  it('offers to reload into the new version', () => {
    const onReload = vi.fn();
    const { getByRole } = render(<UpdateBanner held={false} onReload={onReload} />);
    expect(getByRole('status').textContent).toContain('A new version of Vitela is ready.');
    const reload = getByRole('button', { name: 'Reload' }) as HTMLButtonElement;
    expect(reload.disabled).toBe(false);
    fireEvent.click(reload);
    expect(onReload).toHaveBeenCalledTimes(1);
  });

  it('asks for a save first while there are unsaved changes, and cannot reload', () => {
    const onReload = vi.fn();
    const { getByRole } = render(<UpdateBanner held onReload={onReload} />);
    expect(getByRole('status').textContent).toContain('Save your changes to reload into it.');
    const reload = getByRole('button', { name: 'Reload' }) as HTMLButtonElement;
    expect(reload.disabled).toBe(true);
  });
});
