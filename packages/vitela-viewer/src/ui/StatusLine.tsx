// One line of status text; `tone` only changes its styling and role.

interface StatusLineProps {
  readonly tone?: 'info' | 'error';
  readonly children: string;
}

export function StatusLine({ tone = 'info', children }: StatusLineProps) {
  return (
    <p className={`status-line status-line--${tone}`} role={tone === 'error' ? 'alert' : 'status'}>
      {children}
    </p>
  );
}
