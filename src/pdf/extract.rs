/* pdf/extract.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::{
    get_inherited_rotation, load_document, remove_metadata, remove_outlines, save_document,
};
use lopdf::{Dictionary, Object, ObjectId};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    pub pages: Vec<(usize, u16)>,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

pub fn extract_file<P: AsRef<Path>>(
    file: &(P, u16),
    output_path: P,
    options: &ExtractOptions,
) -> Result<(), PdfError> {
    let (input_path, _) = file;

    let mut doc = load_document(input_path, options.password.as_deref())?;

    let original_pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let original_rotations: Vec<i64> = original_pages
        .iter()
        .map(|&id| get_inherited_rotation(&doc, id))
        .collect();

    let mut new_page_ids = Vec::with_capacity(options.pages.len());
    let mut used_pages = HashSet::new();

    for &(page_idx, rot) in &options.pages {
        let Some(&orig_page_id) = original_pages.get(page_idx) else {
            continue;
        };

        let current_rot = original_rotations[page_idx];
        let total_rot = (current_rot + rot as i64) % 360;

        let target_id = if used_pages.insert(orig_page_id) {
            orig_page_id
        } else if let Ok(Object::Dictionary(dict)) = doc.get_object(orig_page_id) {
            let new_dict = dict.clone();
            let new_id = (doc.max_id + 1, 0);
            doc.max_id += 1;
            doc.objects.insert(new_id, Object::Dictionary(new_dict));
            new_id
        } else {
            orig_page_id
        };

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(target_id) {
            page_dict.set("Rotate", Object::Integer(total_rot));
        }

        new_page_ids.push(target_id);
    }

    let pages_id = (doc.max_id + 1, 0);
    doc.max_id += 1;

    let kids: Vec<Object> = new_page_ids
        .iter()
        .map(|&id| Object::Reference(id))
        .collect();

    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", "Pages");
    pages_dict.set("Kids", kids);
    pages_dict.set("Count", new_page_ids.len() as u32);

    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    for &target_id in &new_page_ids {
        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(target_id) {
            page_dict.set("Parent", pages_id);
        }
    }

    if let Ok(Object::Reference(root_id)) = doc.trailer.get(b"Root")
        && let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(*root_id)
    {
        catalog.set("Pages", pages_id);
    }

    remove_outlines(&mut doc);
    doc.prune_objects();

    if options.remove_metadata {
        remove_metadata(&mut doc);
    }

    save_document(&mut doc, output_path, options.modern_pdf_format)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_utils::create_test_doc;
    use crate::pdf::util::{get_inherited_mediabox, get_inherited_rotation, load_document};
    use lopdf::Object;

    #[test]
    fn test_ext_01_page_reordering_and_subsetting() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_4_pages.pdf");
        let output_path = temp_dir.path().join("extracted_subset.pdf");

        let mut doc = create_test_doc(4, 595.0, 842.0);
        for (idx, page_id) in doc.get_pages().values().enumerate() {
            if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(*page_id) {
                let width = 100.0 * (idx as f32 + 1.0);
                dict.set(
                    "MediaBox",
                    vec![0.into(), 0.into(), width.into(), 842.into()],
                );
                dict.set("PageMarker", idx as i64 + 1);
            }
        }
        doc.save(&input_path).unwrap();

        let options = ExtractOptions {
            pages: vec![(2, 0), (0, 0)],
            ..Default::default()
        };

        extract_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        let out_pages: Vec<ObjectId> = out_doc.get_pages().values().copied().collect();
        assert_eq!(out_pages.len(), 2);

        let page1_dict = out_doc
            .get_object(out_pages[0])
            .and_then(Object::as_dict)
            .unwrap();
        assert_eq!(
            page1_dict
                .get(b"PageMarker")
                .and_then(Object::as_i64)
                .unwrap(),
            3
        );
        assert_eq!(
            get_inherited_mediabox(&out_doc, out_pages[0]).unwrap(),
            vec![0.0, 0.0, 300.0, 842.0]
        );

        let page2_dict = out_doc
            .get_object(out_pages[1])
            .and_then(Object::as_dict)
            .unwrap();
        assert_eq!(
            page2_dict
                .get(b"PageMarker")
                .and_then(Object::as_i64)
                .unwrap(),
            1
        );
        assert_eq!(
            get_inherited_mediabox(&out_doc, out_pages[1]).unwrap(),
            vec![0.0, 0.0, 100.0, 842.0]
        );
    }

    #[test]
    fn test_ext_02_duplicate_page_extraction() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_2_pages.pdf");
        let output_path = temp_dir.path().join("extracted_dup.pdf");

        let mut doc = create_test_doc(2, 595.0, 842.0);
        doc.save(&input_path).unwrap();

        let options = ExtractOptions {
            pages: vec![(0, 0), (0, 90)],
            ..Default::default()
        };

        extract_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        let out_pages: Vec<ObjectId> = out_doc.get_pages().values().copied().collect();
        assert_eq!(out_pages.len(), 2);

        assert_ne!(out_pages[0], out_pages[1]);

        assert_eq!(get_inherited_rotation(&out_doc, out_pages[0]), 0);
        assert_eq!(get_inherited_rotation(&out_doc, out_pages[1]), 90);

        let page2_dict = out_doc
            .get_object(out_pages[1])
            .and_then(Object::as_dict)
            .unwrap();
        assert_eq!(
            page2_dict.get(b"Rotate").and_then(Object::as_i64).unwrap(),
            90
        );
    }

    #[test]
    fn test_ext_03_inherited_rotation_calculation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_inherited_rot.pdf");
        let output_path = temp_dir.path().join("extracted_rot.pdf");

        let mut doc = create_test_doc(1, 595.0, 842.0);
        let page_id = *doc.get_pages().values().next().unwrap();
        let parent_id = match doc.get_object(page_id).and_then(Object::as_dict) {
            Ok(dict) => dict.get(b"Parent").and_then(Object::as_reference).unwrap(),
            _ => panic!("Page must have parent"),
        };
        if let Ok(Object::Dictionary(pages_dict)) = doc.get_object_mut(parent_id) {
            pages_dict.set("Rotate", Object::Integer(90));
        }

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(page_id) {
            assert!(page_dict.get(b"Rotate").is_err());
        }
        assert_eq!(get_inherited_rotation(&doc, page_id), 90);

        doc.save(&input_path).unwrap();

        let options = ExtractOptions {
            pages: vec![(0, 90)],
            ..Default::default()
        };

        extract_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        let out_pages: Vec<ObjectId> = out_doc.get_pages().values().copied().collect();
        assert_eq!(out_pages.len(), 1);

        assert_eq!(get_inherited_rotation(&out_doc, out_pages[0]), 180);
        let page_dict = out_doc
            .get_object(out_pages[0])
            .and_then(Object::as_dict)
            .unwrap();
        assert_eq!(
            page_dict.get(b"Rotate").and_then(Object::as_i64).unwrap(),
            180
        );
    }
}

