/* pdf/metadata.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use lopdf::Document;
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
    _file: &(P, u16),
    _output_path: P,
    options: &MetadataOptions,
) -> Result<(), PdfError> {
    Err(PdfError::Other(format!(
        "Test error. Title: '{}', Author: '{}'",
        options.metadata.title, options.metadata.author
    )))
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
