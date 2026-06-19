/* pdf/split.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DivideAfter {
    #[default]
    EachPage,
    EvenPages,
    OddPages,
}

#[derive(Debug, Clone, Default)]
pub struct SplitOptions {
    pub divide_after: DivideAfter,
    pub prefix: String,
}

pub fn split_file<P: AsRef<Path>>(
    _file: &(P, u16),
    _output_path: P,
    options: &SplitOptions,
) -> Result<(), PdfError> {
    Err(PdfError::Other(format!(
        "Test error. Prefix: '{}', Divide: {:?}",
        options.prefix, options.divide_after
    )))
}
