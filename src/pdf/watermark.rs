/* pdf/watermark.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use crate::pdf::error::PdfError;
use crate::pdf::util::{get_inherited_mediabox, load_document, remove_metadata, save_document};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum WatermarkLayer {
    #[default]
    Front,
    Back,
}

impl From<u32> for WatermarkLayer {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::Back,
            _ => Self::Front,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum WatermarkPages {
    #[default]
    AllPages,
    FirstPage,
    LastPage,
    SpecificPages,
}

impl From<u32> for WatermarkPages {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::FirstPage,
            2 => Self::LastPage,
            3 => Self::SpecificPages,
            _ => Self::AllPages,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WatermarkOptions {
    pub image_path: PathBuf,
    pub layer: WatermarkLayer,
    pub opacity: u32,
    pub pages: WatermarkPages,
    pub specific_pages: Vec<u32>,
    pub modern_pdf_format: bool,
    pub remove_metadata: bool,
    pub password: Option<String>,
}

pub fn watermark_file<P: AsRef<Path>>(
    file: &(P, u16),
    output_path: P,
    options: &WatermarkOptions,
) -> Result<(), PdfError> {
    let (input_path, _) = file;

    let mut doc = load_document(input_path, options.password.as_deref())?;

    let img = image::open(&options.image_path)?;
    let rgba = img.to_rgba8();
    let img_w = rgba.width() as f64;
    let img_h = rgba.height() as f64;

    let mut rgb_bytes = Vec::with_capacity((rgba.width() * rgba.height() * 3) as usize);
    let mut alpha_bytes = Vec::with_capacity((rgba.width() * rgba.height()) as usize);
    let mut has_transparency = false;

    for pixel in rgba.pixels() {
        rgb_bytes.push(pixel[0]);
        rgb_bytes.push(pixel[1]);
        rgb_bytes.push(pixel[2]);
        let a = pixel[3];
        alpha_bytes.push(a);
        if a < 255 {
            has_transparency = true;
        }
    }

    let smask_id = if has_transparency {
        let mut smask_dict = Dictionary::new();
        smask_dict.set("Type", "XObject");
        smask_dict.set("Subtype", "Image");
        smask_dict.set("Width", rgba.width() as i64);
        smask_dict.set("Height", rgba.height() as i64);
        smask_dict.set("ColorSpace", "DeviceGray");
        smask_dict.set("BitsPerComponent", 8);

        let smask_stream = Stream::new(smask_dict, alpha_bytes);
        let id = (doc.max_id + 1, 0);
        doc.max_id += 1;
        doc.objects.insert(id, Object::Stream(smask_stream));
        Some(id)
    } else {
        None
    };

    let mut img_dict = Dictionary::new();
    img_dict.set("Type", "XObject");
    img_dict.set("Subtype", "Image");
    img_dict.set("Width", rgba.width() as i64);
    img_dict.set("Height", rgba.height() as i64);
    img_dict.set("ColorSpace", "DeviceRGB");
    img_dict.set("BitsPerComponent", 8);
    if let Some(sid) = smask_id {
        img_dict.set("SMask", Object::Reference(sid));
    }

    let img_stream = Stream::new(img_dict, rgb_bytes);
    let img_id = (doc.max_id + 1, 0);
    doc.max_id += 1;
    doc.objects.insert(img_id, Object::Stream(img_stream));

    let mut gs_dict = Dictionary::new();
    gs_dict.set("Type", "ExtGState");
    let opacity_val = (options.opacity.min(100) as f32) / 100.0;
    gs_dict.set("ca", Object::Real(opacity_val));
    gs_dict.set("CA", Object::Real(opacity_val));

    let gs_id = (doc.max_id + 1, 0);
    doc.max_id += 1;
    doc.objects.insert(gs_id, Object::Dictionary(gs_dict));

    let img_name = format!("WmImg_{}", img_id.0);
    let gs_name = format!("WmGS_{}", gs_id.0);

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let total_pages = page_ids.len() as u32;

    let target_pages: Vec<u32> = match options.pages {
        WatermarkPages::AllPages => (1..=total_pages).collect(),
        WatermarkPages::FirstPage => {
            if total_pages > 0 {
                vec![1]
            } else {
                vec![]
            }
        }
        WatermarkPages::LastPage => {
            if total_pages > 0 {
                vec![total_pages]
            } else {
                vec![]
            }
        }
        WatermarkPages::SpecificPages => options.specific_pages.clone(),
    };

    let res = WatermarkResource {
        img_id,
        img_name: &img_name,
        img_w,
        img_h,
        gs_id,
        gs_name: &gs_name,
    };

    for page_num in target_pages {
        if let Some(&page_id) = page_ids.get(page_num as usize - 1) {
            apply_watermark_to_page(&mut doc, page_id, &res, options.layer)?;
        }
    }

    if options.remove_metadata {
        remove_metadata(&mut doc);
    }

    doc.compress();

    save_document(&mut doc, output_path, options.modern_pdf_format)?;

    Ok(())
}

struct WatermarkResource<'a> {
    img_id: ObjectId,
    img_name: &'a str,
    img_w: f64,
    img_h: f64,
    gs_id: ObjectId,
    gs_name: &'a str,
}

fn apply_watermark_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    res: &WatermarkResource<'_>,
    layer: WatermarkLayer,
) -> Result<(), PdfError> {
    let media_box =
        get_inherited_mediabox(doc, page_id).unwrap_or_else(|| vec![0.0, 0.0, 595.0, 842.0]);
    let page_w = (media_box[2] - media_box[0]).abs() as f64;
    let page_h = (media_box[3] - media_box[1]).abs() as f64;

    let wm_scale = (page_w / res.img_w).min(page_h / res.img_h).min(1.0);
    let scaled_w = res.img_w * wm_scale;
    let scaled_h = res.img_h * wm_scale;
    let x = media_box[0] as f64 + (page_w - scaled_w) / 2.0;
    let y = media_box[1] as f64 + (page_h - scaled_h) / 2.0;

    let content_str = format!(
        "q\n/{} gs\n{:.4} 0 0 {:.4} {:.4} {:.4} cm\n/{} Do\nQ\n",
        res.gs_name, scaled_w, scaled_h, x, y, res.img_name
    );

    let stream_id = (doc.max_id + 1, 0);
    doc.max_id += 1;
    let stream_obj = Stream::new(Dictionary::new(), content_str.into_bytes());
    doc.objects.insert(stream_id, Object::Stream(stream_obj));

    let page_dict = doc
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .map_err(|_| PdfError::Other("Invalid page dictionary".into()))?;

    match page_dict.get_mut(b"Contents") {
        Ok(Object::Reference(ref_id)) => {
            let new_arr = match layer {
                WatermarkLayer::Back => {
                    vec![Object::Reference(stream_id), Object::Reference(*ref_id)]
                }
                WatermarkLayer::Front => {
                    vec![Object::Reference(*ref_id), Object::Reference(stream_id)]
                }
            };
            page_dict.set("Contents", Object::Array(new_arr));
        }
        Ok(Object::Array(arr)) => match layer {
            WatermarkLayer::Back => arr.insert(0, Object::Reference(stream_id)),
            WatermarkLayer::Front => arr.push(Object::Reference(stream_id)),
        },
        _ => {
            page_dict.set("Contents", Object::Reference(stream_id));
        }
    }

    ensure_page_resources(doc, page_id)?;
    register_resource(doc, page_id, "XObject", res.img_name, res.img_id)?;
    register_resource(doc, page_id, "ExtGState", res.gs_name, res.gs_id)?;

    Ok(())
}

fn get_inherited_resources(doc: &Document, page_id: ObjectId) -> Option<Object> {
    let mut current_id = page_id;
    loop {
        if let Ok(Object::Dictionary(dict)) = doc.get_object(current_id) {
            if let Ok(res) = dict.get(b"Resources") {
                return Some(res.clone());
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

fn ensure_page_resources(doc: &mut Document, page_id: ObjectId) -> Result<(), PdfError> {
    let has_resources = {
        let page_dict = doc
            .get_object(page_id)
            .and_then(Object::as_dict)
            .map_err(|_| PdfError::Other("Invalid page dictionary".into()))?;
        page_dict.get(b"Resources").is_ok()
    };

    if !has_resources {
        let inherited = get_inherited_resources(doc, page_id)
            .unwrap_or_else(|| Object::Dictionary(Dictionary::new()));
        let page_dict = doc
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(|_| PdfError::Other("Invalid page dictionary".into()))?;
        page_dict.set("Resources", inherited);
    }
    Ok(())
}

fn register_resource(
    doc: &mut Document,
    page_id: ObjectId,
    category: &str,
    name: &str,
    obj_id: ObjectId,
) -> Result<(), PdfError> {
    let res_dict_id = {
        let page_dict = doc
            .get_object(page_id)
            .and_then(Object::as_dict)
            .map_err(|_| PdfError::Other("Invalid page dictionary".into()))?;
        match page_dict.get(b"Resources") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    let cat_dict_id = {
        let res_dict = if let Some(id) = res_dict_id {
            doc.get_object(id)
                .and_then(Object::as_dict)
                .map_err(|_| PdfError::Other("Invalid referenced Resources dictionary".into()))?
        } else {
            doc.get_object(page_id)
                .and_then(Object::as_dict)
                .map_err(|_| PdfError::Other("Invalid page dictionary".into()))?
                .get(b"Resources")
                .and_then(Object::as_dict)
                .map_err(|_| PdfError::Other("Missing Resources dictionary".into()))?
        };
        match res_dict.get(category.as_bytes()) {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(id) = cat_dict_id {
        let cat_dict = doc
            .get_object_mut(id)
            .and_then(Object::as_dict_mut)
            .map_err(|_| PdfError::Other("Invalid referenced category dictionary".into()))?;
        cat_dict.set(name, Object::Reference(obj_id));
    } else {
        let res_dict = if let Some(id) = res_dict_id {
            doc.get_object_mut(id)
                .and_then(Object::as_dict_mut)
                .map_err(|_| PdfError::Other("Invalid referenced Resources dictionary".into()))?
        } else {
            doc.get_object_mut(page_id)
                .and_then(Object::as_dict_mut)
                .map_err(|_| PdfError::Other("Invalid page dictionary".into()))?
                .get_mut(b"Resources")
                .and_then(Object::as_dict_mut)
                .map_err(|_| PdfError::Other("Missing Resources dictionary".into()))?
        };

        let cat_bytes = category.as_bytes();
        if res_dict.get(cat_bytes).is_err() {
            res_dict.set(category, Object::Dictionary(Dictionary::new()));
        }
        if let Ok(Object::Dictionary(cat_dict)) = res_dict.get_mut(cat_bytes) {
            cat_dict.set(name, Object::Reference(obj_id));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_utils::create_test_doc;

    fn create_test_image(
        path: &Path,
        width: u32,
        height: u32,
        transparent: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut img = image::RgbaImage::new(width, height);
        let alpha = if transparent { 128 } else { 255 };
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([100, 150, 200, alpha]);
        }
        img.save(path)?;
        Ok(())
    }

    #[test]
    fn test_wtm_01_front_vs_back_layer_routing() {
        let mut doc = create_test_doc(1, 595.0, 842.0);
        let page_id = *doc.get_pages().values().next().unwrap();

        let content_id = (doc.max_id + 1, 0);
        doc.max_id += 1;
        let content_stream = Stream::new(Dictionary::new(), b"q Q\n".to_vec());
        doc.objects.insert(content_id, Object::Stream(content_stream));
        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(page_id) {
            page_dict.set("Contents", Object::Reference(content_id));
        }

        let res_back = WatermarkResource {
            img_id: (100, 0),
            img_name: "WmImg_100",
            img_w: 10.0,
            img_h: 10.0,
            gs_id: (101, 0),
            gs_name: "WmGS_101",
        };
        apply_watermark_to_page(&mut doc, page_id, &res_back, WatermarkLayer::Back).unwrap();

        let page_dict = doc.get_object(page_id).and_then(Object::as_dict).unwrap();
        if let Ok(Object::Array(arr)) = page_dict.get(b"Contents") {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[1], Object::Reference(content_id));
        } else {
            panic!("/Contents must be an Array after applying Back watermark to existing content");
        }

        let mut doc_front = create_test_doc(1, 595.0, 842.0);
        let page_id_front = *doc_front.get_pages().values().next().unwrap();
        let content_id_front = (doc_front.max_id + 1, 0);
        doc_front.max_id += 1;
        let content_stream_front = Stream::new(Dictionary::new(), b"q Q\n".to_vec());
        doc_front
            .objects
            .insert(content_id_front, Object::Stream(content_stream_front));
        if let Ok(Object::Dictionary(page_dict)) = doc_front.get_object_mut(page_id_front) {
            page_dict.set("Contents", Object::Reference(content_id_front));
        }

        let res_front = WatermarkResource {
            img_id: (200, 0),
            img_name: "WmImg_200",
            img_w: 10.0,
            img_h: 10.0,
            gs_id: (201, 0),
            gs_name: "WmGS_201",
        };
        apply_watermark_to_page(&mut doc_front, page_id_front, &res_front, WatermarkLayer::Front)
            .unwrap();

        let page_dict_front = doc_front
            .get_object(page_id_front)
            .and_then(Object::as_dict)
            .unwrap();
        if let Ok(Object::Array(arr)) = page_dict_front.get(b"Contents") {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Object::Reference(content_id_front));
        } else {
            panic!("/Contents must be an Array after applying Front watermark to existing content");
        }
    }

    #[test]
    fn test_wtm_02_alpha_channel_smask_generation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_pdf = temp_dir.path().join("input.pdf");
        let output_trans = temp_dir.path().join("out_trans.pdf");
        let output_opaque = temp_dir.path().join("out_opaque.pdf");
        let img_trans = temp_dir.path().join("trans.png");
        let img_opaque = temp_dir.path().join("opaque.png");

        create_test_doc(1, 595.0, 842.0).save(&input_pdf).unwrap();
        create_test_image(&img_trans, 20, 20, true).unwrap();
        create_test_image(&img_opaque, 20, 20, false).unwrap();

        let opts_trans = WatermarkOptions {
            image_path: img_trans,
            opacity: 100,
            ..Default::default()
        };
        watermark_file(&(input_pdf.clone(), 0), output_trans.clone(), &opts_trans).unwrap();

        let doc_trans = load_document(&output_trans, None).unwrap();
        let mut found_rgb_with_smask = false;
        for obj in doc_trans.objects.values() {
            if let Object::Stream(stream) = obj {
                if let Ok(subtype) = stream.dict.get(b"Subtype").and_then(Object::as_name) {
                    if subtype == b"Image" {
                        if let Ok(cs) = stream.dict.get(b"ColorSpace").and_then(Object::as_name) {
                            if cs == b"DeviceRGB" {
                                if let Ok(Object::Reference(smask_id)) = stream.dict.get(b"SMask") {
                                    found_rgb_with_smask = true;
                                    let smask_obj = doc_trans.get_object(*smask_id).unwrap();
                                    if let Object::Stream(smask_stream) = smask_obj {
                                        assert_eq!(
                                            smask_stream
                                                .dict
                                                .get(b"ColorSpace")
                                                .and_then(Object::as_name)
                                                .unwrap(),
                                            b"DeviceGray"
                                        );
                                    } else {
                                        panic!("SMask object must be a Stream");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(
            found_rgb_with_smask,
            "Must generate and reference an SMask for translucent images"
        );

        let opts_opaque = WatermarkOptions {
            image_path: img_opaque,
            opacity: 100,
            ..Default::default()
        };
        watermark_file(&(input_pdf.clone(), 0), output_opaque.clone(), &opts_opaque).unwrap();

        let doc_opaque = load_document(&output_opaque, None).unwrap();
        for obj in doc_opaque.objects.values() {
            if let Object::Stream(stream) = obj {
                if let Ok(subtype) = stream.dict.get(b"Subtype").and_then(Object::as_name) {
                    if subtype == b"Image" {
                        if let Ok(cs) = stream.dict.get(b"ColorSpace").and_then(Object::as_name) {
                            if cs == b"DeviceRGB" {
                                assert!(
                                    stream.dict.get(b"SMask").is_err(),
                                    "Opaque image must not have SMask"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_wtm_03_page_targeting_routing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input_pdf = temp_dir.path().join("input_5p.pdf");
        let output_first = temp_dir.path().join("out_first.pdf");
        let output_specific = temp_dir.path().join("out_specific.pdf");
        let img_path = temp_dir.path().join("test.png");

        create_test_doc(5, 595.0, 842.0).save(&input_pdf).unwrap();
        create_test_image(&img_path, 10, 10, false).unwrap();

        let opts_first = WatermarkOptions {
            image_path: img_path.clone(),
            pages: WatermarkPages::FirstPage,
            ..Default::default()
        };
        watermark_file(&(input_pdf.clone(), 0), output_first.clone(), &opts_first).unwrap();

        let doc_first = load_document(&output_first, None).unwrap();
        let page_ids_first: Vec<ObjectId> = doc_first.get_pages().values().copied().collect();
        assert_eq!(page_ids_first.len(), 5);

        let page1_dict = doc_first
            .get_object(page_ids_first[0])
            .and_then(Object::as_dict)
            .unwrap();
        assert!(
            page1_dict.get(b"Contents").is_ok(),
            "Page 1 must have /Contents modified"
        );
        for &pid in &page_ids_first[1..5] {
            let p_dict = doc_first.get_object(pid).and_then(Object::as_dict).unwrap();
            assert!(
                p_dict.get(b"Contents").is_err(),
                "Pages 2..=5 must not have /Contents"
            );
        }

        let opts_specific = WatermarkOptions {
            image_path: img_path,
            pages: WatermarkPages::SpecificPages,
            specific_pages: vec![2, 4],
            ..Default::default()
        };
        watermark_file(
            &(input_pdf.clone(), 0),
            output_specific.clone(),
            &opts_specific,
        )
        .unwrap();

        let doc_specific = load_document(&output_specific, None).unwrap();
        let page_ids_spec: Vec<ObjectId> = doc_specific.get_pages().values().copied().collect();
        assert_eq!(page_ids_spec.len(), 5);

        for (idx, &pid) in page_ids_spec.iter().enumerate() {
            let page_num = idx + 1;
            let p_dict = doc_specific
                .get_object(pid)
                .and_then(Object::as_dict)
                .unwrap();
            if page_num == 2 || page_num == 4 {
                assert!(
                    p_dict.get(b"Contents").is_ok(),
                    "Page {} must be watermarked",
                    page_num
                );
            } else {
                assert!(
                    p_dict.get(b"Contents").is_err(),
                    "Page {} must not be watermarked",
                    page_num
                );
            }
        }
    }

    #[test]
    fn test_wtm_04_resource_registration_and_inheritance() {
        let mut doc = create_test_doc(1, 595.0, 842.0);
        let page_id = *doc.get_pages().values().next().unwrap();
        let parent_id = match doc.get_object(page_id).and_then(Object::as_dict) {
            Ok(dict) => dict.get(b"Parent").and_then(Object::as_reference).unwrap(),
            _ => panic!("Page must have Parent"),
        };

        if let Ok(Object::Dictionary(parent_dict)) = doc.get_object_mut(parent_id) {
            let mut inherited_res = Dictionary::new();
            inherited_res.set("Font", Dictionary::new());
            parent_dict.set("Resources", Object::Dictionary(inherited_res));
        }
        if let Ok(Object::Dictionary(page_dict)) = doc.get_object_mut(page_id) {
            page_dict.remove(b"Resources");
            assert!(page_dict.get(b"Resources").is_err());
        }

        let res = WatermarkResource {
            img_id: (300, 0),
            img_name: "WmImg_300",
            img_w: 100.0,
            img_h: 100.0,
            gs_id: (301, 0),
            gs_name: "WmGS_301",
        };

        apply_watermark_to_page(&mut doc, page_id, &res, WatermarkLayer::Front).unwrap();

        let page_dict = doc.get_object(page_id).and_then(Object::as_dict).unwrap();
        let local_res = match page_dict.get(b"Resources") {
            Ok(Object::Dictionary(d)) => d,
            Ok(Object::Reference(rid)) => doc.get_object(*rid).and_then(Object::as_dict).unwrap(),
            _ => panic!("Page must have a local /Resources dictionary after watermarking"),
        };

        let xobj_dict = match local_res.get(b"XObject") {
            Ok(Object::Dictionary(d)) => d,
            Ok(Object::Reference(rid)) => doc.get_object(*rid).and_then(Object::as_dict).unwrap(),
            _ => panic!("/Resources must contain /XObject dictionary"),
        };
        assert_eq!(
            xobj_dict
                .get(b"WmImg_300")
                .and_then(Object::as_reference)
                .unwrap(),
            (300, 0)
        );

        let gs_dict = match local_res.get(b"ExtGState") {
            Ok(Object::Dictionary(d)) => d,
            Ok(Object::Reference(rid)) => doc.get_object(*rid).and_then(Object::as_dict).unwrap(),
            _ => panic!("/Resources must contain /ExtGState dictionary"),
        };
        assert_eq!(
            gs_dict
                .get(b"WmGS_301")
                .and_then(Object::as_reference)
                .unwrap(),
            (301, 0)
        );
    }
}

