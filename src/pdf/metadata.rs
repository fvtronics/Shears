/* pdf/metadata.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct MetadataOptions {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
    pub producer: String,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

pub fn update_metadata<P: AsRef<Path>>(
    _file: &(P, u16),
    _output_path: P,
    options: &MetadataOptions,
) -> Result<(), PdfError> {
    Err(PdfError::Other(format!(
        "Test error. Title: '{}', Author: '{}'",
        options.title, options.author
    )))
}
