/* pdf/organize.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::{get_inherited_rotation, remove_metadata, remove_outlines};
use lopdf::{Dictionary, Document, Object, ObjectId};
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

    let mut doc = if let Some(pass) = &options.password {
        Document::load_with_password(input_path.as_ref(), pass.as_str())?
    } else {
        Document::load(input_path.as_ref())?
    };

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

    if options.modern_pdf_format {
        let mut out_file = std::fs::File::create(output_path.as_ref())?;
        doc.save_modern(&mut out_file)?;
    } else {
        doc.save(output_path.as_ref())?;
    }

    Ok(())
}
