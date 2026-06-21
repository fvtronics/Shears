/* pdf/merge.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

use lopdf::{Bookmark, Document, Object, ObjectId};

use crate::pdf::error::PdfError;
use crate::pdf::util::{apply_file_rotation, remove_metadata};

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
                let d = if let Some(pass) = password {
                    Document::load_with_password(path.as_path(), pass.as_str())?
                } else {
                    Document::load(path.as_path())?
                };
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

    if options.modern_format {
        let mut file = File::create(output_path.as_ref())?;
        merged_doc.save_modern(&mut file)?;
    } else {
        merged_doc.save(output_path.as_ref())?;
    }

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

fn get_inherited_mediabox(doc: &Document, page_id: ObjectId) -> Option<Vec<f32>> {
    let mut current_id = page_id;
    loop {
        if let Ok(Object::Dictionary(dict)) = doc.get_object(current_id) {
            if let Ok(Object::Array(arr)) = dict.get(b"MediaBox")
                && arr.len() == 4
            {
                let get_num = |obj: &Object| -> f32 {
                    match obj {
                        Object::Real(f) => *f,
                        Object::Integer(i) => *i as f32,
                        _ => 0.0,
                    }
                };
                return Some(vec![
                    get_num(&arr[0]),
                    get_num(&arr[1]),
                    get_num(&arr[2]),
                    get_num(&arr[3]),
                ]);
            }
            if let Ok(Object::Reference(parent_id)) = dict.get(b"Parent") {
                current_id = *parent_id;
                continue;
            }
        }
        break;
    }
    None
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
