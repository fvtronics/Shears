/* pdf/mod.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

pub mod error;
pub mod merge;
pub mod metadata;
pub mod preview;
pub mod split;
pub mod util;

pub use error::PdfError;
pub use merge::{MergeOptions, merge_files};
pub use metadata::{MetadataOptions, PdfMetadata, read_metadata, update_metadata};
pub use split::{DivideAfter, SplitOptions, split_file};
