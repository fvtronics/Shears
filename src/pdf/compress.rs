/* pdf/compress.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::{load_document, remove_metadata, save_document};
use image::DynamicImage;
use image::codecs::jpeg::JpegEncoder;
use lopdf::{Document, Object, Stream};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QualityLevel {
    Original,
    Print,
    #[default]
    Display,
    Draft,
}

impl QualityLevel {
    pub fn jpeg_quality(&self) -> Option<u8> {
        match self {
            Self::Original => None,
            Self::Print => Some(90),
            Self::Display => Some(75),
            Self::Draft => Some(50),
        }
    }
}

impl From<u32> for QualityLevel {
    fn from(idx: u32) -> Self {
        match idx {
            0 => Self::Original,
            1 => Self::Print,
            2 => Self::Display,
            3 => Self::Draft,
            _ => Self::Display,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompressOptions {
    pub remove_unused_data: bool,
    pub remove_empty_streams: bool,
    pub image_quality: QualityLevel,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            remove_unused_data: true,
            remove_empty_streams: true,
            image_quality: QualityLevel::Display,
            modern_pdf_format: false,
            remove_metadata: false,
            password: None,
        }
    }
}

pub fn compress_file<P: AsRef<Path>>(
    file: &(P, u16),
    output_path: P,
    options: &CompressOptions,
) -> Result<(), PdfError> {
    let (input_path, _) = file;

    let mut doc = load_document(input_path, options.password.as_deref())?;

    if options.remove_metadata {
        remove_metadata(&mut doc);
    }

    if options.remove_unused_data {
        doc.prune_objects();
    }

    if options.remove_empty_streams {
        doc.delete_zero_length_streams();
    }

    if let Some(quality) = options.image_quality.jpeg_quality() {
        compress_images(&mut doc, quality);
    }

    doc.compress();

    save_document(&mut doc, output_path, options.modern_pdf_format)?;

    Ok(())
}

fn compress_images(doc: &mut Document, quality: u8) {
    for (id, obj) in doc.objects.iter_mut() {
        if let Object::Stream(stream) = obj
            && is_image(stream)
            && let Err(e) = try_compress_stream(*id, stream, quality)
        {
            tracing::debug!("[img {:?}] compression failed: {}", id, e);
        }
    }
}

fn try_compress_stream(
    id: lopdf::ObjectId,
    stream: &mut Stream,
    quality: u8,
) -> Result<(), PdfError> {
    if stream.dict.get(b"Mask").is_ok() || stream.dict.get(b"SMask").is_ok() {
        tracing::debug!("[img {:?}] skipped: masking or soft mask present", id);
        return Ok(());
    }

    if stream
        .dict
        .get(b"BitsPerComponent")
        .and_then(Object::as_i64)
        .is_ok_and(|bits| bits != 8)
    {
        return Ok(());
    }

    let cs = stream
        .dict
        .get(b"ColorSpace")
        .and_then(Object::as_name)
        .ok();
    if !matches!(cs, Some(b"DeviceGray" | b"DeviceRGB")) {
        return Ok(());
    }

    let filter = match stream.dict.get(b"Filter") {
        Ok(Object::Name(n)) => Some(n.as_slice()),
        Ok(Object::Array(arr)) if arr.len() == 1 => arr[0].as_name().ok(),
        Ok(Object::Array(_)) => return Ok(()),
        _ => None,
    };

    let decoded_image = match filter {
        Some(b"DCTDecode" | b"JPXDecode") => image::load_from_memory(&stream.content).ok(),
        _ => {
            let width = stream
                .dict
                .get(b"Width")
                .and_then(Object::as_i64)
                .unwrap_or(0) as u32;
            let height = stream
                .dict
                .get(b"Height")
                .and_then(Object::as_i64)
                .unwrap_or(0) as u32;
            if width == 0 || height == 0 {
                return Ok(());
            }
            let raw_bytes = stream.decompressed_content()?;
            if cs == Some(b"DeviceGray") {
                image::GrayImage::from_raw(width, height, raw_bytes).map(DynamicImage::ImageLuma8)
            } else {
                image::RgbImage::from_raw(width, height, raw_bytes).map(DynamicImage::ImageRgb8)
            }
        }
    };

    let Some(image) = decoded_image else {
        return Ok(());
    };

    let mut jpeg_bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg_bytes, quality).encode_image(&image)?;

    if !jpeg_bytes.is_empty() && jpeg_bytes.len() < stream.content.len() {
        tracing::debug!(
            "[img {:?}] compressed {}B -> {}B",
            id,
            stream.content.len(),
            jpeg_bytes.len()
        );
        stream.content = jpeg_bytes;
        stream
            .dict
            .set(b"Filter", Object::Name(b"DCTDecode".to_vec()));
        stream.dict.remove(b"DecodeParms");
        let target_cs = match image {
            DynamicImage::ImageLuma8(_) => b"DeviceGray".as_slice(),
            _ => b"DeviceRGB".as_slice(),
        };
        stream
            .dict
            .set(b"ColorSpace", Object::Name(target_cs.to_vec()));
        stream
            .dict
            .set(b"Length", Object::Integer(stream.content.len() as i64));
    }

    Ok(())
}

fn is_image(stream: &Stream) -> bool {
    stream
        .dict
        .get(b"Subtype")
        .and_then(Object::as_name)
        .is_ok_and(|subtype| subtype == b"Image")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_utils::{create_doc_with_image_stream, create_test_doc};
    use crate::pdf::util::load_document;
    use lopdf::{Dictionary, Object, Stream};

    #[test]
    fn test_cmp_01_uncompressed_rgb_jpeg_encoding() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_uncompressed_img.pdf");
        let output_path = temp_dir.path().join("compressed_img.pdf");

        let mut doc = create_doc_with_image_stream(100, 100);
        let img_id = doc
            .objects
            .iter()
            .find(|(_, obj)| obj.as_stream().is_ok_and(is_image))
            .map(|(id, _)| *id)
            .unwrap();
        let orig_len = match doc.objects.get(&img_id).unwrap() {
            Object::Stream(s) => s.content.len(),
            _ => unreachable!(),
        };
        doc.save(&input_path).unwrap();

        let options = CompressOptions {
            image_quality: QualityLevel::Display,
            ..Default::default()
        };
        compress_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        let out_stream = match out_doc.objects.get(&img_id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("Expected stream object"),
        };

        assert_eq!(
            out_stream.dict.get(b"Filter").and_then(Object::as_name).ok(),
            Some(b"DCTDecode".as_slice())
        );
        assert_eq!(
            out_stream
                .dict
                .get(b"ColorSpace")
                .and_then(Object::as_name)
                .ok(),
            Some(b"DeviceRGB".as_slice())
        );
        assert!(out_stream.content.len() < orig_len);
    }

    #[test]
    fn test_cmp_02_transparency_mask_skip_rule() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_smask_img.pdf");
        let output_path = temp_dir.path().join("compressed_smask.pdf");

        let mut doc = create_doc_with_image_stream(100, 100);
        let img_id = doc
            .objects
            .iter()
            .find(|(_, obj)| obj.as_stream().is_ok_and(is_image))
            .map(|(id, _)| *id)
            .unwrap();

        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&img_id) {
            stream.dict.set("SMask", Object::Reference((99, 0)));
        }
        doc.save(&input_path).unwrap();

        let options = CompressOptions {
            image_quality: QualityLevel::Display,
            ..Default::default()
        };
        compress_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        let out_stream = match out_doc.objects.get(&img_id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("Expected stream object"),
        };

        assert_eq!(
            out_stream.dict.get(b"Filter").and_then(Object::as_name).ok(),
            Some(b"FlateDecode".as_slice())
        );
    }

    #[test]
    fn test_cmp_03_non_8_bit_component_skip_rule() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_4bit_img.pdf");
        let output_path = temp_dir.path().join("compressed_4bit.pdf");

        let mut doc = create_doc_with_image_stream(100, 100);
        let img_id = doc
            .objects
            .iter()
            .find(|(_, obj)| obj.as_stream().is_ok_and(is_image))
            .map(|(id, _)| *id)
            .unwrap();

        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&img_id) {
            stream.dict.set("BitsPerComponent", Object::Integer(4));
        }
        doc.save(&input_path).unwrap();

        let options = CompressOptions {
            image_quality: QualityLevel::Display,
            ..Default::default()
        };
        compress_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        let out_stream = match out_doc.objects.get(&img_id).unwrap() {
            Object::Stream(s) => s,
            _ => panic!("Expected stream object"),
        };

        assert_eq!(
            out_stream.dict.get(b"Filter").and_then(Object::as_name).ok(),
            Some(b"FlateDecode".as_slice())
        );
        assert_eq!(
            out_stream
                .dict
                .get(b"BitsPerComponent")
                .and_then(Object::as_i64)
                .ok(),
            Some(4)
        );
    }

    #[test]
    fn test_cmp_04_zero_length_stream_and_unused_pruning() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_path = temp_dir.path().join("input_prune.pdf");
        let output_path = temp_dir.path().join("compressed_pruned.pdf");

        let mut doc = create_test_doc(1, 595.0, 842.0);
        let unref_id = (100, 0);
        let mut dummy_dict = Dictionary::new();
        dummy_dict.set("Type", "UnreferencedDummy");
        doc.objects.insert(unref_id, Object::Dictionary(dummy_dict));

        let zero_id = (101, 0);
        let mut stream_dict = Dictionary::new();
        stream_dict.set("Length", 0);
        let empty_stream = Stream::new(stream_dict, Vec::new());
        doc.objects.insert(zero_id, Object::Stream(empty_stream));

        let page_id = *doc.get_pages().values().next().unwrap();
        if let Some(Object::Dictionary(page_dict)) = doc.objects.get_mut(&page_id) {
            let mut xobjects = Dictionary::new();
            xobjects.set("ZeroStream", Object::Reference(zero_id));
            let mut res = Dictionary::new();
            res.set("XObject", Object::Dictionary(xobjects));
            page_dict.set("Resources", Object::Dictionary(res));
        }

        doc.save(&input_path).unwrap();

        let options = CompressOptions {
            remove_unused_data: true,
            remove_empty_streams: true,
            ..Default::default()
        };
        compress_file(&(input_path.clone(), 0), output_path.clone(), &options).unwrap();

        let out_doc = load_document(&output_path, None).unwrap();
        assert!(!out_doc.objects.contains_key(&unref_id));
        assert!(!out_doc.objects.contains_key(&zero_id));
    }
}
