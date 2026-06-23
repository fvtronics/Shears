/* pdf/metadata.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::remove_metadata;
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

        if options.modern_pdf_format {
            let mut out_file = fs::File::create(output_path.as_ref())?;
            doc.save_modern(&mut out_file)?;
        } else {
            doc.save(output_path.as_ref())?;
        }
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

fn apply_metadata(
    info_dict: &mut Dictionary,
    metadata: &PdfMetadata,
    producer: &str,
) {
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
