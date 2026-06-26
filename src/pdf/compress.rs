/* pdf/compress.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct CompressOptions {
    pub remove_unused_data: bool,
    pub remove_empty_streams: bool,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

pub fn compress_file<P: AsRef<Path>>(
    _file: &(P, u16),
    _output_path: P,
    options: &CompressOptions,
) -> Result<(), PdfError> {
    Err(PdfError::Other(format!(
        "Test error. Remove unused data: {}, Remove empty streams: {}",
        options.remove_unused_data, options.remove_empty_streams
    )))
}
