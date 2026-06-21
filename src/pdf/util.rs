/* pdf/util.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use lopdf::{Document, Object, ObjectId};

pub fn get_inherited_rotation(doc: &Document, page_id: ObjectId) -> i64 {
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

pub fn apply_file_rotation(doc: &mut Document, rotation: u16) {
    let pages = doc.get_pages();
    for page_id in pages.values() {
        let current_rotation = get_inherited_rotation(doc, *page_id);
        let new_rotation = (current_rotation + rotation as i64) % 360;

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(*page_id) {
            page_dict.set("Rotate", Object::Integer(new_rotation));
        }
    }
}

pub fn remove_metadata(doc: &mut Document) {
    doc.trailer.remove(b"Info");
    if let Ok(Object::Reference(root_id)) = doc.trailer.get(b"Root")
        && let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(*root_id)
    {
        catalog.remove(b"Metadata");
    }
}

pub fn remove_outlines(doc: &mut Document) {
    if let Ok(Object::Reference(root_id)) = doc.trailer.get(b"Root")
        && let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(*root_id)
    {
        catalog.remove(b"Outlines");
    }
}
