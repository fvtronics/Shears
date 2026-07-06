/* pdf/mod.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

pub mod compress;
pub mod error;
pub mod extract;
pub mod merge;
pub mod metadata;
pub mod organize;
pub mod preview;
pub mod split;
pub mod util;

pub use compress::{CompressOptions, QualityLevel, compress_file};
pub use error::PdfError;
pub use extract::{ExtractOptions, extract_file};
pub use merge::{MergeOptions, merge_files};
pub use metadata::{MetadataOptions, PdfMetadata, read_metadata, update_metadata};
pub use organize::{OrganizeOptions, OrganizePageInput, organize_file};
pub use split::{DivideAfter, SplitOptions, split_file};
