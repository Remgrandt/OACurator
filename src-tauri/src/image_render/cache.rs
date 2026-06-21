// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use crate::image_render::recipe::OutputFormat;

pub fn cache_extension_for_output(format: &OutputFormat, has_alpha: bool) -> &'static str {
    match format {
        OutputFormat::Png => "png",
        OutputFormat::Jpeg { .. } if has_alpha => "png",
        OutputFormat::Jpeg { .. } => "jpg",
    }
}
