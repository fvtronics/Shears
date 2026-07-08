/* pdf/split.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::{load_document, remove_metadata, remove_outlines, save_document};
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

    let doc = load_document(input_path, options.password.as_deref())?;

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

    save_document(&mut split_doc, &out_file_path, options.modern_format)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_utils::create_test_doc;
    use crate::pdf::util::load_document;
    use lopdf::{Dictionary, Object};

    #[test]
    fn test_spl_01_divide_after_every_n_pages() {
        let segments = calculate_segments(&DivideAfter::EveryNPages(3), 10);
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0], vec![1, 2, 3]);
        assert_eq!(segments[1], vec![4, 5, 6]);
        assert_eq!(segments[2], vec![7, 8, 9]);
        assert_eq!(segments[3], vec![10]);
    }

    #[test]
    fn test_spl_02_divide_after_specific_pages() {
        let segments = calculate_segments(&DivideAfter::SpecificPages(vec![2, 5]), 6);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0], vec![1, 2]);
        assert_eq!(segments[1], vec![3, 4, 5]);
        assert_eq!(segments[2], vec![6]);
    }

    #[test]
    fn test_spl_03_divide_even_odd_pages() {
        let even_segments = calculate_segments(&DivideAfter::EvenPages, 5);
        assert_eq!(even_segments, vec![vec![1, 2], vec![3, 4], vec![5]]);

        let odd_segments = calculate_segments(&DivideAfter::OddPages, 5);
        assert_eq!(odd_segments, vec![vec![1], vec![2, 3], vec![4, 5]]);
    }

    #[test]
    fn test_spl_04_outline_pruning_and_bloat_prevention() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_10_pages.pdf");

        let mut doc = create_test_doc(10, 595.0, 842.0);
        let root_id = doc.trailer.get(b"Root").and_then(Object::as_reference).unwrap();
        let cat = doc.get_object_mut(root_id).and_then(Object::as_dict_mut).unwrap();
        cat.set("Outlines", (100, 0));

        let mut outline_dict = Dictionary::new();
        outline_dict.set("Type", "Outlines");
        doc.objects.insert((100, 0), Object::Dictionary(outline_dict));

        let mut dummy_dict = Dictionary::new();
        dummy_dict.set("Type", "DummyUnreferenced");
        doc.objects.insert((101, 0), Object::Dictionary(dummy_dict));

        let orig_obj_count = doc.objects.len();
        doc.save(&input_path).unwrap();

        let options = SplitOptions {
            divide_after: DivideAfter::SpecificPages(vec![1]),
            prefix: "split_test".to_string(),
            ..Default::default()
        };
        split_file(&(input_path, 0), temp_dir.path().to_path_buf(), &options).unwrap();

        let out_path = temp_dir.path().join("split_test_001.pdf");
        assert!(out_path.exists());

        let split_doc = load_document(&out_path, None).unwrap();
        assert_eq!(split_doc.get_pages().len(), 1);

        let split_root_id = split_doc
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .unwrap();
        let split_cat = split_doc
            .get_object(split_root_id)
            .and_then(Object::as_dict)
            .unwrap();
        assert!(split_cat.get(b"Outlines").is_err());

        assert!(split_doc.objects.len() < orig_obj_count);
        assert!(!split_doc.objects.contains_key(&(101, 0)));
    }
}
