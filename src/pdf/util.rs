/* pdf/util.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::fs::File;
use std::path::Path;

use lopdf::{Document, Object, ObjectId};

use crate::pdf::error::PdfError;

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

pub fn get_inherited_mediabox(doc: &Document, page_id: ObjectId) -> Option<Vec<f32>> {
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

pub fn load_document<P: AsRef<Path>>(
    path: P,
    password: Option<&str>,
) -> Result<Document, PdfError> {
    if let Some(pass) = password {
        Ok(Document::load_with_password(path.as_ref(), pass)?)
    } else {
        Ok(Document::load(path.as_ref())?)
    }
}

pub fn save_document<P: AsRef<Path>>(
    doc: &mut Document,
    output_path: P,
    modern_format: bool,
) -> Result<(), PdfError> {
    if modern_format {
        let mut out_file = File::create(output_path.as_ref())?;
        doc.save_modern(&mut out_file)?;
    } else {
        doc.save(output_path.as_ref())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_utils::{add_page_node, add_pages_node, create_test_doc, set_root_catalog};
    use lopdf::Object;

    #[test]
    fn test_utl_01_deeply_nested_inherited_rotation() {
        let mut doc = Document::with_version("1.5");
        let master_pages_id = add_pages_node(&mut doc, None, Some(90), None);
        set_root_catalog(&mut doc, master_pages_id);

        let sub_pages_id = add_pages_node(&mut doc, Some(master_pages_id), Some(180), None);
        let page_id = add_page_node(&mut doc, sub_pages_id, Some(vec![0.0, 0.0, 595.0, 842.0]));

        let rotation = get_inherited_rotation(&doc, page_id);
        assert_eq!(rotation, 180);
    }

    #[test]
    fn test_get_inherited_rotation_direct_and_default() {
        let mut doc = create_test_doc(1, 595.0, 842.0);
        let page_id = (3, 0);

        assert_eq!(get_inherited_rotation(&doc, page_id), 0);

        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(page_id) {
            page_dict.set("Rotate", 270);
        }
        assert_eq!(get_inherited_rotation(&doc, page_id), 270);
    }

    #[test]
    fn test_utl_02_deeply_nested_inherited_mediabox() {
        let mut doc = Document::with_version("1.5");
        let pages_id = add_pages_node(&mut doc, None, None, Some(vec![0.0, 0.0, 612.0, 792.0]));
        set_root_catalog(&mut doc, pages_id);

        let page_id = add_page_node(&mut doc, pages_id, None);

        let mediabox = get_inherited_mediabox(&doc, page_id);
        assert_eq!(mediabox, Some(vec![0.0, 0.0, 612.0, 792.0]));
    }

    #[test]
    fn test_get_inherited_mediabox_direct_and_missing() {
        let mut doc = create_test_doc(1, 500.0, 700.0);
        let page_id = (3, 0);

        assert_eq!(
            get_inherited_mediabox(&doc, page_id),
            Some(vec![0.0, 0.0, 500.0, 700.0])
        );

        let pages_id = (2, 0);
        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(page_id) {
            page_dict.remove(b"MediaBox");
        }
        if let Ok(Object::Dictionary(pages_dict)) = doc.get_object_mut(pages_id) {
            pages_dict.remove(b"MediaBox");
        }
        assert_eq!(get_inherited_mediabox(&doc, page_id), None);
    }

    #[test]
    fn test_utl_03_cumulative_rotation_application() {
        let mut doc = create_test_doc(2, 595.0, 842.0);
        let page1_id = (3, 0);
        let page2_id = (4, 0);

        if let Ok(Object::Dictionary(p1)) = doc.get_object_mut(page1_id) {
            p1.set("Rotate", 270);
        }
        if let Ok(Object::Dictionary(p2)) = doc.get_object_mut(page2_id) {
            p2.set("Rotate", 270);
        }

        apply_file_rotation(&mut doc, 90);

        assert_eq!(get_inherited_rotation(&doc, page1_id), 0);
        assert_eq!(get_inherited_rotation(&doc, page2_id), 0);

        apply_file_rotation(&mut doc, 180);
        assert_eq!(get_inherited_rotation(&doc, page1_id), 180);
        assert_eq!(get_inherited_rotation(&doc, page2_id), 180);
    }

    #[test]
    fn test_remove_metadata() {
        let mut doc = create_test_doc(1, 595.0, 842.0);
        let info_id = (10, 0);
        doc.trailer.set("Info", info_id);

        let root_id = (1, 0);
        let metadata_id = (11, 0);
        if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(root_id) {
            catalog.set("Metadata", metadata_id);
        }

        remove_metadata(&mut doc);

        assert!(doc.trailer.get(b"Info").is_err());
        let catalog = doc.get_object(root_id).and_then(Object::as_dict).unwrap();
        assert!(catalog.get(b"Metadata").is_err());
    }

    #[test]
    fn test_remove_outlines() {
        let mut doc = create_test_doc(1, 595.0, 842.0);
        let root_id = (1, 0);
        let outlines_id = (12, 0);
        if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(root_id) {
            catalog.set("Outlines", outlines_id);
        }

        remove_outlines(&mut doc);

        let catalog = doc.get_object(root_id).and_then(Object::as_dict).unwrap();
        assert!(catalog.get(b"Outlines").is_err());
    }

    #[test]
    fn test_load_and_save_document() {
        let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
        let modern_path = temp_dir.path().join("test_modern.pdf");
        let legacy_path = temp_dir.path().join("test_legacy.pdf");

        let mut doc = create_test_doc(2, 595.0, 842.0);

        save_document(&mut doc, &modern_path, true).expect("Failed to save modern document");
        assert!(modern_path.exists());

        save_document(&mut doc, &legacy_path, false).expect("Failed to save legacy document");
        assert!(legacy_path.exists());

        let loaded_modern =
            load_document(&modern_path, None).expect("Failed to load modern document");
        assert_eq!(loaded_modern.get_pages().len(), 2);

        let loaded_legacy =
            load_document(&legacy_path, None).expect("Failed to load legacy document");
        assert_eq!(loaded_legacy.get_pages().len(), 2);

        let non_existent = temp_dir.path().join("does_not_exist.pdf");
        assert!(load_document(&non_existent, None).is_err());
    }
}
