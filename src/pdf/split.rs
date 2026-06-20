/* pdf/split.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DivideAfter {
    #[default]
    EachPage,
    EvenPages,
    OddPages,
    EveryNPages(u32),
    SpecificPages(Vec<std::ops::RangeInclusive<u32>>),
}

#[derive(Debug, Clone, Default)]
pub struct SplitOptions {
    pub divide_after: DivideAfter,
    pub prefix: String,
    pub password: Option<String>,
    pub modern_format: bool,
    pub remove_metadata: bool,
}

pub fn split_file<P: AsRef<Path>>(
    _file: &(P, u16),
    _output_path: P,
    options: &SplitOptions,
) -> Result<(), PdfError> {
    Err(PdfError::Other(format!(
        "Test error. Prefix: '{}', Divide: {:?}, Pass: {:?}, Modern: {:?}, Metadata: {:?}",
        options.prefix,
        options.divide_after,
        options.password.is_some(),
        options.modern_format,
        options.remove_metadata
    )))
}
