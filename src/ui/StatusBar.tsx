type StatusBarProps = {
  status: string;
};

export function StatusBar({ status }: StatusBarProps) {
  return (
    <footer className="statusbar" role="contentinfo" aria-label="Workbench Status">
      <span className="status-primary">{status}</span>
    </footer>
  );
}
