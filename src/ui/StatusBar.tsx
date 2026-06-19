// Copyright (c) 2026 Remgrandt Works. All rights reserved.

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
