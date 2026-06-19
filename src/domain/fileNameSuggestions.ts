// Copyright (c) 2026 Remgrandt Works. All rights reserved.

// Frontend cleanup is suggestion-only. Rust path safety remains authoritative.
export function suggestedExportFileStem(value: string) {
  return (
    value
      .trim()
      // eslint-disable-next-line no-control-regex -- Windows filenames cannot contain ASCII control characters.
      .replace(/[<>:"/\\|?*\u0000-\u001f]+/g, " ")
      .replace(/\s+/g, " ")
      .replace(/[. ]+$/g, "")
  );
}
