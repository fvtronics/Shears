/* pdf/extract.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    pub pages: Vec<(usize, u16)>,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

pub fn extract_file<P: AsRef<Path>>(
    _file: &(P, u16),
    _output_path: P,
    options: &ExtractOptions,
) -> Result<(), PdfError> {
    Err(PdfError::Other(format!(
        "Test error. Extracted pages count: {}, Modern format: {}, Remove metadata: {}, Has password: {}",
        options.pages.len(),
        options.modern_pdf_format,
        options.remove_metadata,
        options.password.is_some()
    )))
}
