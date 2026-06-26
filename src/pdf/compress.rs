/* pdf/compress.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::remove_metadata;
use lopdf::Document;
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
    file: &(P, u16),
    output_path: P,
    options: &CompressOptions,
) -> Result<(), PdfError> {
    let (input_path, _) = file;

    let mut doc = if let Some(pass) = &options.password {
        Document::load_with_password(input_path.as_ref(), pass.as_str())?
    } else {
        Document::load(input_path.as_ref())?
    };

    if options.remove_metadata {
        remove_metadata(&mut doc);
    }

    if options.remove_unused_data {
        doc.prune_objects();
    }

    if options.remove_empty_streams {
        doc.delete_zero_length_streams();
    }

    doc.compress();

    if options.modern_pdf_format {
        let mut file = std::fs::File::create(output_path.as_ref())?;
        doc.save_modern(&mut file)?;
    } else {
        doc.save(output_path.as_ref())?;
    }

    Ok(())
}
