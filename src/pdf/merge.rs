/* pdf/merge.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    pub modern_format: bool,
    pub normalize_page_size: bool,
    pub remove_metadata: bool,
}

pub fn merge_files<P: AsRef<Path>>(
    _files: &[(P, u16)],
    _output_path: P,
    _options: &MergeOptions,
) -> Result<(), PdfError> {
    Ok(())
}
