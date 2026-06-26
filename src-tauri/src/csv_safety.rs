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

#[cfg(test)]
mod tests {
    use super::spreadsheet_safe_cell;

    #[test]
    fn spreadsheet_safe_cell_prefixes_formula_starters() {
        for value in [
            "=1+1",
            "+1+1",
            "-1+1",
            "@SUM(1,2)",
            "\t=1+1",
            "\r=1+1",
            "\n=1+1",
        ] {
            assert_eq!(spreadsheet_safe_cell(value), format!("'{value}"));
        }
    }

    #[test]
    fn spreadsheet_safe_cell_leaves_normal_values_unchanged() {
        for value in [
            "Raremarq Ready",
            "https://example.com/image.jpg",
            "275",
            "",
            " in stock",
        ] {
            assert_eq!(spreadsheet_safe_cell(value), value);
        }
    }
}
