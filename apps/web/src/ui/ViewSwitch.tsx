// Switches between the screens of the open document. Presentational: a group
// of toggle buttons, the current one pressed.

interface ViewSwitchProps<View extends string> {
  readonly views: readonly { readonly value: View; readonly label: string }[];
  readonly current: View;
  readonly onChange: (view: View) => void;
}

export function ViewSwitch<View extends string>({ views, current, onChange }: ViewSwitchProps<View>) {
  return (
    <div className="view-switch" role="group" aria-label="Screen">
      {views.map(({ value, label }) => (
        <button key={value} type="button" aria-pressed={value === current} onClick={() => onChange(value)}>
          {label}
        </button>
      ))}
    </div>
  );
}
