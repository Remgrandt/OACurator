// Copyright (c) 2026 Remgrandt Works. All rights reserved.

pub fn spreadsheet_safe_cell(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    if has_spreadsheet_formula_prefix(value) {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

fn has_spreadsheet_formula_prefix(value: &str) -> bool {
    matches!(
        value.as_bytes().first(),
        Some(b'=' | b'+' | b'-' | b'@' | b'\t' | b'\r' | b'\n')
    )
}
