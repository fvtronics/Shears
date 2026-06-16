/* pdf/mod.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

pub mod error;
pub mod merge;

pub use error::PdfError;
pub use merge::{MergeOptions, merge_files};
