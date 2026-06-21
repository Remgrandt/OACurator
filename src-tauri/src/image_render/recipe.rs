// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderRecipe {
    pub version: u32,
    pub resize: ResizeMode,
    pub output: OutputFormat,
    pub color: ColorPolicy,
    pub alpha: AlphaPolicy,
    pub filter: ResizeFilter,
    pub allow_upscale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResizeMode {
    FitWithin {
        max_width: Option<u32>,
        max_height: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputFormat {
    Png,
    Jpeg { quality: u8 },
}

impl OutputFormat {
    pub fn format_name(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg { .. } => "jpg",
        }
    }

    pub fn extension(&self) -> &'static str {
        self.format_name()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ColorPolicy {
    Preserve,
    ConvertToSrgb8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlphaPolicy {
    Preserve,
    FlattenOnBackground { r: u8, g: u8, b: u8 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResizeFilter {
    Nearest,
    Triangle,
    Lanczos3,
}

pub fn thumbnail_recipe() -> RenderRecipe {
    RenderRecipe {
        version: 1,
        resize: ResizeMode::FitWithin {
            max_width: Some(320),
            max_height: Some(320),
        },
        output: OutputFormat::Jpeg { quality: 82 },
        color: ColorPolicy::ConvertToSrgb8,
        alpha: AlphaPolicy::Preserve,
        filter: ResizeFilter::Lanczos3,
        allow_upscale: false,
    }
}

pub fn preview_recipe() -> RenderRecipe {
    RenderRecipe {
        version: 1,
        resize: ResizeMode::FitWithin {
            max_width: None,
            max_height: Some(2000),
        },
        output: OutputFormat::Jpeg { quality: 88 },
        color: ColorPolicy::ConvertToSrgb8,
        alpha: AlphaPolicy::Preserve,
        filter: ResizeFilter::Lanczos3,
        allow_upscale: false,
    }
}

pub fn basic_png_export_recipe() -> RenderRecipe {
    RenderRecipe {
        version: 1,
        resize: ResizeMode::FitWithin {
            max_width: None,
            max_height: Some(800),
        },
        output: OutputFormat::Png,
        color: ColorPolicy::ConvertToSrgb8,
        alpha: AlphaPolicy::Preserve,
        filter: ResizeFilter::Lanczos3,
        allow_upscale: false,
    }
}

pub fn premium_png_export_recipe() -> RenderRecipe {
    RenderRecipe {
        version: 1,
        resize: ResizeMode::FitWithin {
            max_width: None,
            max_height: Some(2000),
        },
        output: OutputFormat::Png,
        color: ColorPolicy::ConvertToSrgb8,
        alpha: AlphaPolicy::Preserve,
        filter: ResizeFilter::Lanczos3,
        allow_upscale: false,
    }
}

pub fn raremarq_upload_jpeg_recipe(max_dimension: u32, quality: u8) -> RenderRecipe {
    RenderRecipe {
        version: 1,
        resize: ResizeMode::FitWithin {
            max_width: Some(max_dimension),
            max_height: Some(max_dimension),
        },
        output: OutputFormat::Jpeg { quality },
        color: ColorPolicy::ConvertToSrgb8,
        alpha: AlphaPolicy::FlattenOnBackground {
            r: 255,
            g: 255,
            b: 255,
        },
        filter: ResizeFilter::Lanczos3,
        allow_upscale: false,
    }
}
