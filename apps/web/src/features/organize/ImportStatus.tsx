// What "Add PDFs" is doing, as a status line, plus the password prompt of a
// locked file. Presentational: the pick itself is `useImport`'s.

import { StatusLine } from '../../ui/StatusLine';
import { PasswordDialog } from '../document/PasswordDialog';
import type { ImportActivity } from './useImport';

interface ImportStatusProps {
  readonly activity: ImportActivity;
  readonly onSubmitPassword: (password: string) => void;
  readonly onCancelPassword: () => void;
}

export function ImportStatus({ activity, onSubmitPassword, onCancelPassword }: ImportStatusProps) {
  switch (activity.kind) {
    case 'preparing':
      return <StatusLine>{`Reading ${activity.name}…`}</StatusLine>;
    case 'importing':
      return <StatusLine>Adding the pages…</StatusLine>;
    case 'added':
      return <StatusLine>{activity.message}</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{activity.message}</StatusLine>;
    case 'password':
      return (
        <PasswordDialog
          action="add"
          name={activity.name}
          wrong={activity.wrong}
          checking={activity.checking}
          onSubmit={onSubmitPassword}
          onCancel={onCancelPassword}
        />
      );
    case 'idle':
      return null;
  }
}
