/* pdf/organize.rs
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

#[derive(Debug, Clone)]
pub enum OrganizePageInput {
    Page(usize),
    BlankPage { width: f64, height: f64 },
}

#[derive(Debug, Clone, Default)]
pub struct OrganizeOptions {
    pub pages: Vec<(OrganizePageInput, u16)>,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

pub fn organize_file<P: AsRef<Path>>(
    file: &(P, u16),
    output_path: P,
    options: &OrganizeOptions,
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

    for (item, rot) in &options.pages {
        match item {
            OrganizePageInput::Page(page_idx) => {
                let Some(&orig_page_id) = original_pages.get(*page_idx) else {
                    continue;
                };

                let current_rot = original_rotations[*page_idx];
                let total_rot = (current_rot + *rot as i64) % 360;

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
            OrganizePageInput::BlankPage { width, height } => {
                let mut page_dict = Dictionary::new();
                page_dict.set("Type", "Page");
                page_dict.set(
                    "MediaBox",
                    vec![
                        0.into(),
                        0.into(),
                        (*width as f32).into(),
                        (*height as f32).into(),
                    ],
                );
                if *rot != 0 {
                    page_dict.set("Rotate", Object::Integer(*rot as i64));
                }
                page_dict.set("Resources", Dictionary::new());

                let target_id = (doc.max_id + 1, 0);
                doc.max_id += 1;
                doc.objects.insert(target_id, Object::Dictionary(page_dict));
                new_page_ids.push(target_id);
            }
        }
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
    fn test_org_01_mixed_reordering_and_blank_insertion() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_3_pages.pdf");
        let output_path = temp_dir.path().join("organized_mixed.pdf");

        let mut doc = create_test_doc(3, 595.0, 842.0);
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

        let options = OrganizeOptions {
            pages: vec![
                (OrganizePageInput::Page(2), 0),
                (
                    OrganizePageInput::BlankPage {
                        width: 600.0,
                        height: 800.0,
                    },
                    90,
                ),
                (OrganizePageInput::Page(0), 0),
            ],
            ..Default::default()
        };

        organize_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        let out_pages: Vec<ObjectId> = out_doc.get_pages().values().copied().collect();
        assert_eq!(out_pages.len(), 3);

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

        let middle_dict = out_doc
            .get_object(out_pages[1])
            .and_then(Object::as_dict)
            .unwrap();
        assert!(middle_dict.get(b"PageMarker").is_err());
        assert_eq!(
            get_inherited_mediabox(&out_doc, out_pages[1]).unwrap(),
            vec![0.0, 0.0, 600.0, 800.0]
        );
        assert_eq!(get_inherited_rotation(&out_doc, out_pages[1]), 90);
        assert_eq!(
            middle_dict.get(b"Rotate").and_then(Object::as_i64).unwrap(),
            90
        );

        let page3_dict = out_doc
            .get_object(out_pages[2])
            .and_then(Object::as_dict)
            .unwrap();
        assert_eq!(
            page3_dict
                .get(b"PageMarker")
                .and_then(Object::as_i64)
                .unwrap(),
            1
        );
        assert_eq!(
            get_inherited_mediabox(&out_doc, out_pages[2]).unwrap(),
            vec![0.0, 0.0, 100.0, 842.0]
        );
    }

    #[test]
    fn test_org_02_duplicate_reference_cloning() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_2_pages.pdf");
        let output_path = temp_dir.path().join("organized_dup.pdf");

        let mut doc = create_test_doc(2, 595.0, 842.0);
        doc.save(&input_path).unwrap();

        let options = OrganizeOptions {
            pages: vec![
                (OrganizePageInput::Page(1), 0),
                (OrganizePageInput::Page(1), 180),
            ],
            ..Default::default()
        };

        organize_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        let out_pages: Vec<ObjectId> = out_doc.get_pages().values().copied().collect();
        assert_eq!(out_pages.len(), 2);

        assert_ne!(out_pages[0], out_pages[1]);

        assert_eq!(get_inherited_rotation(&out_doc, out_pages[0]), 0);
        assert_eq!(get_inherited_rotation(&out_doc, out_pages[1]), 180);

        let page2_dict = out_doc
            .get_object(out_pages[1])
            .and_then(Object::as_dict)
            .unwrap();
        assert_eq!(
            page2_dict.get(b"Rotate").and_then(Object::as_i64).unwrap(),
            180
        );
    }
}

