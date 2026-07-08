/* pdf/metadata.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::{remove_metadata, save_document};
use lopdf::{Dictionary, Document, IncrementalDocument, Object, StringFormat};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct MetadataOptions {
    pub metadata: PdfMetadata,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PdfMetadata {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
    pub producer: String,
}

pub fn update_metadata<P: AsRef<Path>>(
    file: &(P, u16),
    output_path: P,
    options: &MetadataOptions,
) -> Result<(), PdfError> {
    let (input_path, _) = file;
    let bytes = fs::read(input_path.as_ref())?;
    let mut doc = if let Some(pass) = &options.password {
        Document::load_mem_with_options(&bytes, lopdf::LoadOptions::with_password(pass.as_str()))?
    } else {
        Document::load_mem(&bytes)?
    };

    let producer = format!("Quire {}", env!("CARGO_PKG_VERSION"));

    if options.modern_pdf_format || options.remove_metadata {
        if options.remove_metadata {
            remove_metadata(&mut doc);
        }

        let mut info_dict = get_existing_info(&doc);

        apply_metadata(&mut info_dict, &options.metadata, &producer);
        insert_info_dict(&mut doc, info_dict);

        save_document(&mut doc, output_path, options.modern_pdf_format)?;
    } else {
        let mut inc_doc = IncrementalDocument::create_from(bytes, doc);

        let mut info_dict = get_existing_info(inc_doc.get_prev_documents());
        apply_metadata(&mut info_dict, &options.metadata, &producer);
        insert_info_dict(&mut inc_doc.new_document, info_dict);

        inc_doc.save(output_path.as_ref())?;
    }

    Ok(())
}

pub fn read_metadata<P: AsRef<Path>>(
    file_path: P,
    password: Option<&str>,
) -> Result<PdfMetadata, PdfError> {
    let metadata = if let Some(pwd) = password {
        Document::load_metadata_with_password(file_path, pwd)?
    } else {
        Document::load_metadata(file_path)?
    };

    Ok(PdfMetadata {
        title: metadata.title.unwrap_or_default(),
        author: metadata.author.unwrap_or_default(),
        subject: metadata.subject.unwrap_or_default(),
        keywords: metadata.keywords.unwrap_or_default(),
        creator: metadata.creator.unwrap_or_default(),
        producer: metadata.producer.unwrap_or_default(),
    })
}

fn encode_pdf_string(text: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF]; // BOM for UTF-16BE
    for utf16_char in text.encode_utf16() {
        bytes.extend_from_slice(&utf16_char.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Literal)
}

fn set_or_remove_string(dict: &mut Dictionary, key: &[u8], val: &str) {
    if val.is_empty() {
        dict.remove(key);
    } else {
        dict.set(key, encode_pdf_string(val));
    }
}

fn get_existing_info(doc: &Document) -> Dictionary {
    match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => doc
            .get_object(*id)
            .ok()
            .and_then(|obj| obj.as_dict().ok())
            .cloned()
            .unwrap_or_else(Dictionary::new),
        Ok(Object::Dictionary(dict)) => dict.clone(),
        _ => Dictionary::new(),
    }
}

fn apply_metadata(info_dict: &mut Dictionary, metadata: &PdfMetadata, producer: &str) {
    set_or_remove_string(info_dict, b"Title", &metadata.title);
    set_or_remove_string(info_dict, b"Author", &metadata.author);
    set_or_remove_string(info_dict, b"Subject", &metadata.subject);
    set_or_remove_string(info_dict, b"Keywords", &metadata.keywords);
    info_dict.set("Producer", encode_pdf_string(producer));
}

fn insert_info_dict(doc: &mut Document, info_dict: Dictionary) {
    if let Ok(Object::Reference(id)) = doc.trailer.get(b"Info") {
        doc.objects.insert(*id, Object::Dictionary(info_dict));
    } else {
        doc.max_id += 1;
        let info_id = (doc.max_id, 0);
        doc.objects.insert(info_id, Object::Dictionary(info_dict));
        doc.trailer.set("Info", Object::Reference(info_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_utils::create_test_doc;
    use lopdf::{Dictionary, Object, StringFormat};
    use std::fs;

    #[test]
    fn test_met_01_utf16be_bom_string_encoding() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input.pdf");
        let output_path = temp_dir.path().join("output_utf16.pdf");

        create_test_doc(1, 595.0, 842.0).save(&input_path).unwrap();

        let options = MetadataOptions {
            metadata: PdfMetadata {
                title: "Reporte año 2026".to_string(),
                author: "José".to_string(),
                ..Default::default()
            },
            modern_pdf_format: true,
            remove_metadata: false,
            password: None,
        };
        update_metadata(&(input_path, 0), output_path.clone(), &options).unwrap();

        let loaded_meta = read_metadata(&output_path, None).unwrap();
        assert_eq!(loaded_meta.title, "Reporte año 2026");
        assert_eq!(loaded_meta.author, "José");

        let doc = Document::load(&output_path).unwrap();
        let info_id = doc
            .trailer
            .get(b"Info")
            .and_then(Object::as_reference)
            .unwrap();
        let info_dict = doc.get_object(info_id).and_then(Object::as_dict).unwrap();

        let Object::String(title_bytes, title_fmt) = info_dict.get(b"Title").unwrap() else {
            panic!("Title must be Object::String");
        };
        assert_eq!(*title_fmt, StringFormat::Literal);
        assert_eq!(&title_bytes[0..2], &[0xFE, 0xFF]);
        let expected_utf16: Vec<u8> = "Reporte año 2026"
            .encode_utf16()
            .flat_map(|c| c.to_be_bytes())
            .collect();
        assert_eq!(&title_bytes[2..], &expected_utf16);
    }

    #[test]
    fn test_met_02_metadata_scrubbing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_with_meta.pdf");
        let output_path = temp_dir.path().join("output_scrubbed.pdf");

        let mut doc = create_test_doc(1, 595.0, 842.0);
        let mut info_dict = Dictionary::new();
        info_dict.set("Title", encode_pdf_string("Old Title"));
        info_dict.set("Author", encode_pdf_string("Old Author"));
        info_dict.set("Subject", encode_pdf_string("Old Subject"));
        info_dict.set("Keywords", encode_pdf_string("Old Keywords"));
        doc.objects.insert((10, 0), Object::Dictionary(info_dict));
        doc.trailer.set("Info", (10, 0));
        doc.save(&input_path).unwrap();

        let options = MetadataOptions {
            remove_metadata: true,
            modern_pdf_format: true,
            ..Default::default()
        };
        update_metadata(&(input_path, 0), output_path.clone(), &options).unwrap();

        let loaded_meta = read_metadata(&output_path, None).unwrap();
        assert!(loaded_meta.title.is_empty());
        assert!(loaded_meta.author.is_empty());
        assert!(loaded_meta.subject.is_empty());
        assert!(loaded_meta.keywords.is_empty());
        assert!(loaded_meta.producer.starts_with("Quire"));

        let out_doc = Document::load(&output_path).unwrap();
        let info_id = out_doc
            .trailer
            .get(b"Info")
            .and_then(Object::as_reference)
            .unwrap();
        let out_info = out_doc.get_object(info_id).and_then(Object::as_dict).unwrap();
        assert!(out_info.get(b"Title").is_err());
        assert!(out_info.get(b"Author").is_err());
        assert!(out_info.get(b"Subject").is_err());
        assert!(out_info.get(b"Keywords").is_err());
    }

    #[test]
    fn test_met_03_incremental_document_save_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_base.pdf");
        let output_path = temp_dir.path().join("output_inc.pdf");

        create_test_doc(1, 595.0, 842.0).save(&input_path).unwrap();
        let orig_bytes = fs::read(&input_path).unwrap();

        let options = MetadataOptions {
            metadata: PdfMetadata {
                title: "Incremental Title".to_string(),
                ..Default::default()
            },
            modern_pdf_format: false,
            remove_metadata: false,
            password: None,
        };
        update_metadata(&(input_path, 0), output_path.clone(), &options).unwrap();

        let inc_bytes = fs::read(&output_path).unwrap();
        assert!(inc_bytes.len() > orig_bytes.len());
        assert!(inc_bytes.starts_with(&orig_bytes));

        let loaded_meta = read_metadata(&output_path, None).unwrap();
        assert_eq!(loaded_meta.title, "Incremental Title");
    }
}
