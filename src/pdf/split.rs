/* pdf/split.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::{remove_metadata, remove_outlines};
use lopdf::Document;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DivideAfter {
    #[default]
    EachPage,
    EvenPages,
    OddPages,
    EveryNPages(u32),
    SpecificPages(Vec<u32>),
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
    file: &(P, u16),
    output_path: P,
    options: &SplitOptions,
) -> Result<(), PdfError> {
    let (input_path, _) = file;

    let doc = if let Some(pass) = &options.password {
        Document::load_with_password(input_path.as_ref(), pass.as_str())?
    } else {
        Document::load(input_path.as_ref())?
    };

    let total_pages = doc.get_pages().len() as u32;
    let segments = calculate_segments(&options.divide_after, total_pages);
    let base_out_dir = output_path.as_ref();

    for (idx, segment) in segments.iter().enumerate() {
        extract_document(&doc, segment, options, idx, base_out_dir)?;
    }

    Ok(())
}

fn calculate_segments(divide_after: &DivideAfter, total_pages: u32) -> Vec<Vec<u32>> {
    let mut segments: Vec<Vec<u32>> = Vec::new();
    let mut current_segment = Vec::new();

    for p in 1..=total_pages {
        current_segment.push(p);

        let should_cut = match divide_after {
            DivideAfter::EachPage => true,
            DivideAfter::EvenPages => p % 2 == 0,
            DivideAfter::OddPages => p % 2 == 1,
            DivideAfter::EveryNPages(n) => p % (*n).max(1) == 0,
            DivideAfter::SpecificPages(pages) => pages.contains(&p),
        };

        if should_cut {
            segments.push(current_segment);
            current_segment = Vec::new();
        }
    }

    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    segments
}

fn extract_document(
    doc: &Document,
    segment: &[u32],
    options: &SplitOptions,
    idx: usize,
    base_out_dir: &Path,
) -> Result<(), PdfError> {
    let mut split_doc = doc.clone();

    let all_pages: Vec<u32> = split_doc.get_pages().keys().copied().collect();
    let pages_to_delete: Vec<u32> = all_pages
        .into_iter()
        .filter(|p| !segment.contains(p))
        .collect();

    split_doc.delete_pages(&pages_to_delete);
    remove_outlines(&mut split_doc);
    split_doc.prune_objects();

    if options.remove_metadata {
        remove_metadata(&mut split_doc);
    }

    let out_filename = format!("{}_{:03}.pdf", options.prefix, idx + 1);
    let out_file_path = base_out_dir.join(out_filename);

    if options.modern_format {
        let mut out_file = std::fs::File::create(&out_file_path)?;
        split_doc.save_modern(&mut out_file)?;
    } else {
        split_doc.save(&out_file_path)?;
    }

    Ok(())
}
