/* pdf/merge.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::path::Path;
use std::path::PathBuf;

use lopdf::{Bookmark, Document, Object};

use crate::pdf::error::PdfError;
use crate::pdf::util::{
    apply_file_rotation, get_inherited_mediabox, load_document, remove_metadata, save_document,
};

#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    pub modern_format: bool,
    pub normalize_page_size: bool,
    pub remove_metadata: bool,
}

#[derive(Debug, Clone)]
pub enum MergeInput {
    File(PathBuf, Option<String>),
    BlankPage {
        title: String,
        width: f64,
        height: f64,
    },
}

pub fn merge_files<P: AsRef<Path>>(
    files: &[(MergeInput, u16)],
    output_path: P,
    options: &MergeOptions,
) -> Result<(), PdfError> {
    let mut documents = Vec::with_capacity(files.len());

    for (input, rotation) in files {
        let (filename, mut doc) = match input {
            MergeInput::File(path, password) => {
                let d = load_document(path.as_path(), password.as_deref())?;
                let fname = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                (fname, d)
            }
            MergeInput::BlankPage {
                title,
                width,
                height,
            } => (
                title.clone(),
                create_blank_pdf(*width as f32, *height as f32),
            ),
        };

        if *rotation != 0 {
            apply_file_rotation(&mut doc, *rotation);
        }

        documents.push((filename, doc));
    }

    let mut merged_doc = merge_documents(documents)?;

    if options.remove_metadata {
        remove_metadata(&mut merged_doc);
    }

    if options.normalize_page_size {
        normalize_page_sizes(&mut merged_doc);
    }

    save_document(&mut merged_doc, output_path, options.modern_format)?;

    Ok(())
}

fn create_blank_pdf(width: f32, height: f32) -> Document {
    let mut doc = Document::with_version("1.5");

    let catalog_id = (1, 0);
    let pages_id = (2, 0);
    let page_id = (3, 0);

    doc.max_id = 3;

    let mut catalog = lopdf::Dictionary::new();
    catalog.set("Type", "Catalog");
    catalog.set("Pages", pages_id);
    doc.objects.insert(catalog_id, Object::Dictionary(catalog));

    let mut pages = lopdf::Dictionary::new();
    pages.set("Type", "Pages");
    pages.set("Kids", vec![Object::Reference(page_id)]);
    pages.set("Count", 1);
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut page = lopdf::Dictionary::new();
    page.set("Type", "Page");
    page.set("Parent", pages_id);
    page.set(
        "MediaBox",
        vec![0.into(), 0.into(), width.into(), height.into()],
    );
    page.set("Resources", lopdf::Dictionary::new());
    doc.objects.insert(page_id, Object::Dictionary(page));

    doc.trailer.set("Root", catalog_id);

    doc
}

fn normalize_page_sizes(doc: &mut Document) {
    let pages = doc.get_pages();

    let (max_width, max_height) = pages
        .values()
        .fold((0.0_f32, 0.0_f32), |(mw, mh), &page_id| {
            if let Some(media_box) = get_inherited_mediabox(doc, page_id) {
                let w = (media_box[2] - media_box[0]).abs();
                let h = (media_box[3] - media_box[1]).abs();
                (mw.max(w), mh.max(h))
            } else {
                (mw, mh)
            }
        });

    if max_width > 0.0 && max_height > 0.0 {
        for page_id in pages.values() {
            let original_box = get_inherited_mediabox(doc, *page_id)
                .unwrap_or_else(|| vec![0.0, 0.0, max_width, max_height]);

            let llx = original_box[0];
            let lly = original_box[1];

            let new_media_box = vec![
                Object::Real(llx),
                Object::Real(lly),
                Object::Real(llx + max_width),
                Object::Real(lly + max_height),
            ];

            if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(*page_id) {
                page_dict.set("MediaBox", Object::Array(new_media_box));
                page_dict.remove(b"CropBox");
                page_dict.remove(b"TrimBox");
                page_dict.remove(b"BleedBox");
                page_dict.remove(b"ArtBox");
            }
        }
    }
}

fn merge_documents(documents: Vec<(String, Document)>) -> Result<Document, PdfError> {
    let mut document = Document::with_version("1.5");
    let mut max_id = 1;
    let mut master_kids = Vec::new();
    let mut total_page_count = 0;

    for (filename, mut doc) in documents {
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        let catalog_id = doc
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .map_err(|_| PdfError::Other("Missing Root in trailer".into()))?;

        let catalog = doc
            .get_object(catalog_id)
            .and_then(Object::as_dict)
            .map_err(|_| PdfError::Other("Invalid Catalog".into()))?;

        let pages_id = catalog
            .get(b"Pages")
            .and_then(Object::as_reference)
            .map_err(|_| PdfError::Other("Missing Pages in Catalog".into()))?;

        let pages_dict = doc
            .get_object(pages_id)
            .and_then(Object::as_dict)
            .map_err(|_| PdfError::Other("Invalid Pages node".into()))?;

        let count = pages_dict
            .get(b"Count")
            .and_then(Object::as_i64)
            .unwrap_or(0) as u32;

        total_page_count += count;
        master_kids.push(Object::Reference(pages_id));

        if let Some((_, first_page_id)) = doc.get_pages().into_iter().next() {
            let title = if filename.is_empty() {
                format!("Document {}", master_kids.len())
            } else {
                filename
            };

            let bookmark = Bookmark::new(title, [0.0, 0.0, 1.0], 0, first_page_id);
            document.add_bookmark(bookmark, None);
        }

        document.objects.extend(doc.objects);
    }

    document.max_id = max_id;
    let master_pages_id = (document.max_id + 1, 0);
    let master_catalog_id = (document.max_id + 2, 0);
    document.max_id += 2;

    let mut master_pages_dict = lopdf::Dictionary::new();
    master_pages_dict.set("Type", "Pages");
    master_pages_dict.set("Kids", master_kids.clone());
    master_pages_dict.set("Count", total_page_count);

    document
        .objects
        .insert(master_pages_id, Object::Dictionary(master_pages_dict));

    for kid in master_kids {
        if let Object::Reference(kid_id) = kid
            && let Ok(Object::Dictionary(dict)) = document.get_object_mut(kid_id)
        {
            dict.set("Parent", master_pages_id);
        }
    }

    let mut master_catalog_dict = lopdf::Dictionary::new();
    master_catalog_dict.set("Type", "Catalog");
    master_catalog_dict.set("Pages", master_pages_id);

    if let Some(outline_id) = document.build_outline() {
        master_catalog_dict.set("Outlines", Object::Reference(outline_id));
    }

    document
        .objects
        .insert(master_catalog_id, Object::Dictionary(master_catalog_dict));

    document.trailer.set("Root", master_catalog_id);
    document.compress();

    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_utils::create_test_doc;
    use crate::pdf::util::{get_inherited_mediabox, get_inherited_rotation, load_document};
    use lopdf::Object;

    #[test]
    fn test_mrg_01_page_size_normalization() {
        let mut doc_a = create_test_doc(2, 595.0, 842.0);
        for page_id in doc_a.get_pages().values() {
            if let Ok(Object::Dictionary(dict)) = doc_a.get_object_mut(*page_id) {
                dict.set("CropBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
                dict.set("TrimBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
                dict.set("BleedBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
                dict.set("ArtBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
            }
        }
        let doc_b = create_test_doc(1, 842.0, 1191.0);

        let mut merged =
            merge_documents(vec![("DocA".into(), doc_a), ("DocB".into(), doc_b)]).unwrap();

        normalize_page_sizes(&mut merged);

        for page_id in merged.get_pages().values() {
            let box_coords = get_inherited_mediabox(&merged, *page_id).unwrap();
            assert_eq!(box_coords, vec![0.0, 0.0, 842.0, 1191.0]);

            let dict = merged
                .get_object(*page_id)
                .and_then(Object::as_dict)
                .unwrap();
            assert!(dict.get(b"CropBox").is_err());
            assert!(dict.get(b"TrimBox").is_err());
            assert!(dict.get(b"BleedBox").is_err());
            assert!(dict.get(b"ArtBox").is_err());
        }
    }

    #[test]
    fn test_mrg_02_explicit_rotation_application() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc_a_path = temp_dir.path().join("doc_a.pdf");
        let output_path = temp_dir.path().join("merged.pdf");

        let mut doc_a = create_test_doc(1, 595.0, 842.0);
        if let Some((_, page_id)) = doc_a.get_pages().into_iter().next()
            && let Ok(Object::Dictionary(dict)) = doc_a.get_object_mut(page_id)
        {
            dict.set("Rotate", 90);
        }
        doc_a.save(&doc_a_path).unwrap();

        let options = MergeOptions::default();
        merge_files(
            &[(MergeInput::File(doc_a_path, None), 90)],
            &output_path,
            &options,
        )
        .unwrap();

        let merged = load_document(&output_path, None).unwrap();
        let (_, page_id) = merged.get_pages().into_iter().next().unwrap();
        assert_eq!(get_inherited_rotation(&merged, page_id), 180);
    }

    #[test]
    fn test_mrg_03_blank_page_interleaving() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc_a_path = temp_dir.path().join("doc_a.pdf");
        let doc_b_path = temp_dir.path().join("doc_b.pdf");
        let output_path = temp_dir.path().join("merged_interleaved.pdf");

        create_test_doc(2, 595.0, 842.0).save(&doc_a_path).unwrap();
        create_test_doc(1, 595.0, 842.0).save(&doc_b_path).unwrap();

        let inputs = vec![
            (MergeInput::File(doc_a_path, None), 0),
            (
                MergeInput::BlankPage {
                    title: "Blank".into(),
                    width: 500.0,
                    height: 500.0,
                },
                0,
            ),
            (MergeInput::File(doc_b_path, None), 0),
        ];

        let options = MergeOptions::default();
        merge_files(&inputs, &output_path, &options).unwrap();

        let merged = load_document(&output_path, None).unwrap();
        let pages = merged.get_pages();
        assert_eq!(pages.len(), 4);

        let third_page_id = *pages.get(&3).expect("Page 3 should exist");
        let box_coords = get_inherited_mediabox(&merged, third_page_id).unwrap();
        assert_eq!(box_coords, vec![0.0, 0.0, 500.0, 500.0]);

        let dict = merged
            .get_object(third_page_id)
            .and_then(Object::as_dict)
            .unwrap();
        assert!(dict.get(b"Resources").is_ok());
    }

    #[test]
    fn test_mrg_04_master_catalog_and_bookmarks() {
        let doc1 = create_test_doc(2, 595.0, 842.0);
        let doc2 = create_test_doc(1, 595.0, 842.0);
        let doc3 = create_test_doc(3, 595.0, 842.0);

        let merged = merge_documents(vec![
            ("Seg 1".into(), doc1),
            ("Seg 2".into(), doc2),
            ("Seg 3".into(), doc3),
        ])
        .unwrap();

        let root_id = merged
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .unwrap();
        let catalog = merged
            .get_object(root_id)
            .and_then(Object::as_dict)
            .unwrap();
        let outlines_id = catalog
            .get(b"Outlines")
            .and_then(Object::as_reference)
            .unwrap();
        let outlines = merged
            .get_object(outlines_id)
            .and_then(Object::as_dict)
            .unwrap();

        let mut current_item_id = outlines
            .get(b"First")
            .and_then(Object::as_reference)
            .unwrap();
        let mut titles = Vec::new();

        loop {
            if let Ok(Object::Dictionary(item_dict)) = merged.get_object(current_item_id) {
                if let Ok(Object::String(title_bytes, _)) = item_dict.get(b"Title") {
                    titles.push(String::from_utf8_lossy(title_bytes).to_string());
                }
                if let Ok(Object::Reference(next_id)) = item_dict.get(b"Next") {
                    current_item_id = *next_id;
                    continue;
                }
            }
            break;
        }

        assert_eq!(titles, vec!["Seg 1", "Seg 2", "Seg 3"]);
    }

    #[test]
    fn test_mrg_05_metadata_stripping_on_merge() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc1_path = temp_dir.path().join("doc1.pdf");
        let doc2_path = temp_dir.path().join("doc2.pdf");
        let output_path = temp_dir.path().join("merged_no_meta.pdf");

        let mut doc1 = create_test_doc(1, 595.0, 842.0);
        doc1.trailer.set("Info", (10, 0));
        let root1 = doc1
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .unwrap();
        let cat1 = doc1
            .get_object_mut(root1)
            .and_then(Object::as_dict_mut)
            .unwrap();
        cat1.set("Metadata", (11, 0));
        doc1.save(&doc1_path).unwrap();

        let mut doc2 = create_test_doc(1, 595.0, 842.0);
        doc2.trailer.set("Info", (12, 0));
        let root2 = doc2
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .unwrap();
        let cat2 = doc2
            .get_object_mut(root2)
            .and_then(Object::as_dict_mut)
            .unwrap();
        cat2.set("Metadata", (13, 0));
        doc2.save(&doc2_path).unwrap();

        let options = MergeOptions {
            remove_metadata: true,
            ..Default::default()
        };
        merge_files(
            &[
                (MergeInput::File(doc1_path, None), 0),
                (MergeInput::File(doc2_path, None), 0),
            ],
            &output_path,
            &options,
        )
        .unwrap();

        let merged = load_document(&output_path, None).unwrap();
        assert!(merged.trailer.get(b"Info").is_err());
        let root_id = merged
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .unwrap();
        let cat = merged
            .get_object(root_id)
            .and_then(Object::as_dict)
            .unwrap();
        assert!(cat.get(b"Metadata").is_err());
    }
}
