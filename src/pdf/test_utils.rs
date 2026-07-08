/* pdf/test_utils.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use lopdf::{Dictionary, Document, Object, ObjectId};

pub fn create_test_doc(page_count: u32, width: f32, height: f32) -> Document {
    let mut doc = Document::with_version("1.5");
    let catalog_id = (1, 0);
    let pages_id = (2, 0);
    doc.max_id = 2;

    let mut kids = Vec::with_capacity(page_count as usize);
    for _ in 0..page_count {
        let page_id = (doc.max_id + 1, 0);
        doc.max_id += 1;

        let mut page = Dictionary::new();
        page.set("Type", "Page");
        page.set("Parent", pages_id);
        page.set(
            "MediaBox",
            vec![0.into(), 0.into(), width.into(), height.into()],
        );
        page.set("Resources", Dictionary::new());
        doc.objects.insert(page_id, Object::Dictionary(page));
        kids.push(Object::Reference(page_id));
    }

    let mut pages = Dictionary::new();
    pages.set("Type", "Pages");
    pages.set("Kids", kids);
    pages.set("Count", page_count);
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", "Catalog");
    catalog.set("Pages", pages_id);
    doc.objects.insert(catalog_id, Object::Dictionary(catalog));
    doc.trailer.set("Root", catalog_id);

    doc
}

pub fn add_pages_node(
    doc: &mut Document,
    parent: Option<ObjectId>,
    rotate: Option<i64>,
    mediabox: Option<Vec<f32>>,
) -> ObjectId {
    let id = (doc.max_id + 1, 0);
    doc.max_id += 1;

    let mut dict = Dictionary::new();
    dict.set("Type", "Pages");
    if let Some(p) = parent {
        dict.set("Parent", p);
    }
    dict.set("Count", 0);
    dict.set("Kids", Vec::<Object>::new());
    if let Some(rot) = rotate {
        dict.set("Rotate", rot);
    }
    if let Some(mb) = mediabox {
        dict.set(
            "MediaBox",
            mb.into_iter().map(Object::Real).collect::<Vec<_>>(),
        );
    }
    doc.objects.insert(id, Object::Dictionary(dict));

    if let Some(p) = parent
        && let Ok(Object::Dictionary(parent_dict)) = doc.get_object_mut(p)
        && let Ok(Object::Array(kids)) = parent_dict.get_mut(b"Kids")
    {
        kids.push(Object::Reference(id));
    }

    id
}

pub fn add_page_node(
    doc: &mut Document,
    parent: ObjectId,
    mediabox: Option<Vec<f32>>,
) -> ObjectId {
    let id = (doc.max_id + 1, 0);
    doc.max_id += 1;

    let mut dict = Dictionary::new();
    dict.set("Type", "Page");
    dict.set("Parent", parent);
    if let Some(mb) = mediabox {
        dict.set(
            "MediaBox",
            mb.into_iter().map(Object::Real).collect::<Vec<_>>(),
        );
    }
    dict.set("Resources", Dictionary::new());
    doc.objects.insert(id, Object::Dictionary(dict));

    let mut current = parent;
    if let Ok(Object::Dictionary(parent_dict)) = doc.get_object_mut(current)
        && let Ok(Object::Array(kids)) = parent_dict.get_mut(b"Kids")
    {
        kids.push(Object::Reference(id));
    }

    loop {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(current) {
            if let Ok(count) = dict.get(b"Count").and_then(Object::as_i64) {
                dict.set("Count", count + 1);
            }
            if let Ok(Object::Reference(p)) = dict.get(b"Parent").cloned() {
                current = p;
                continue;
            }
        }
        break;
    }

    id
}

pub fn set_root_catalog(doc: &mut Document, pages_id: ObjectId) -> ObjectId {
    let id = (doc.max_id + 1, 0);
    doc.max_id += 1;

    let mut dict = Dictionary::new();
    dict.set("Type", "Catalog");
    dict.set("Pages", pages_id);
    doc.objects.insert(id, Object::Dictionary(dict));
    doc.trailer.set("Root", id);
    id
}
