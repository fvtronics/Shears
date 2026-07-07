/* pdf/watermark.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum WatermarkLayer {
    #[default]
    Front,
    Back,
}

impl From<u32> for WatermarkLayer {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::Back,
            _ => Self::Front,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum WatermarkPages {
    #[default]
    AllPages,
    FirstPage,
    LastPage,
    SpecificPages,
}

impl From<u32> for WatermarkPages {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::FirstPage,
            2 => Self::LastPage,
            3 => Self::SpecificPages,
            _ => Self::AllPages,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WatermarkOptions {
    pub image_path: PathBuf,
    pub layer: WatermarkLayer,
    pub opacity: u32,
    pub pages: WatermarkPages,
    pub specific_pages: String,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

pub fn watermark_file<P: AsRef<Path>>(
    _file: &(P, u16),
    _output_path: P,
    options: &WatermarkOptions,
) -> Result<(), PdfError> {
    Err(PdfError::Other(format!(
        "Test error. Image path: {:?}, Layer: {:?}, Opacity: {}, Pages: {:?}, Specific pages: {}, Modern format: {}, Remove metadata: {}, Has password: {}",
        options.image_path,
        options.layer,
        options.opacity,
        options.pages,
        options.specific_pages,
        options.modern_pdf_format,
        options.remove_metadata,
        options.password.is_some()
    )))
}
