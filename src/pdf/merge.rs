/* pdf/merge.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::path::Path;

use lopdf::{Bookmark, Document, Object, ObjectId};

use crate::pdf::error::PdfError;

#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    pub modern_format: bool,
    pub normalize_page_size: bool,
    pub remove_metadata: bool,
}

pub fn merge_files<P: AsRef<Path>>(
    files: &[(P, u16)],
    output_path: P,
    _options: &MergeOptions,
) -> Result<(), PdfError> {
    let mut documents = Vec::with_capacity(files.len());

    for (path, rotation) in files {
        let mut doc = Document::load(path.as_ref())?;
        if *rotation != 0 {
            apply_file_rotation(&mut doc, *rotation);
        }
        let filename = path
            .as_ref()
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        documents.push((filename, doc));
    }

    let mut merged_doc = merge_documents(documents)?;

    merged_doc.save(output_path.as_ref())?;

    Ok(())
}

fn get_inherited_rotation(doc: &Document, page_id: ObjectId) -> i64 {
    let mut current_id = page_id;
    loop {
        if let Ok(Object::Dictionary(dict)) = doc.get_object(current_id) {
            if let Ok(rotate) = dict.get(b"Rotate").and_then(Object::as_i64) {
                return rotate;
            }
            if let Ok(Object::Reference(parent_id)) = dict.get(b"Parent") {
                current_id = *parent_id;
                continue;
            }
        }
        break;
    }
    0
}

fn apply_file_rotation(doc: &mut Document, rotation: u16) {
    let pages = doc.get_pages();
    for page_id in pages.values() {
        let current_rotation = get_inherited_rotation(doc, *page_id);
        let new_rotation = (current_rotation + rotation as i64) % 360;

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(*page_id) {
            page_dict.set("Rotate", Object::Integer(new_rotation));
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
