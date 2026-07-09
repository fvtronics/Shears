use relm4::gtk::cairo;
use relm4::gtk::{gdk, gio, glib, prelude::FileExt};
use std::sync::OnceLock;

pub fn thread_pool() -> &'static glib::ThreadPool {
    static POOL: OnceLock<glib::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| glib::ThreadPool::exclusive(4).expect("Failed to create thread pool"))
}

#[derive(Debug)]
pub enum PreviewError {
    Encrypted,
    Other,
}

#[derive(Debug)]
pub struct ThumbnailResult {
    pub texture: Option<gdk::MemoryTexture>,
    pub original_dimensions: Option<(f64, f64)>,
    pub page_count: i32,
}

fn render_to_texture(
    width: i32,
    height: i32,
    render_fn: impl FnOnce(&cairo::Context),
) -> Option<gdk::MemoryTexture> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|e| tracing::error!("Failed to create surface: {:?}", e))
        .ok()?;
    let cr = cairo::Context::new(&surface)
        .map_err(|e| tracing::error!("Failed to create context: {:?}", e))
        .ok()?;

    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.paint()
        .map_err(|e| tracing::error!("Failed to paint background: {:?}", e))
        .ok()?;

    render_fn(&cr);

    add_border(&cr, width, height);

    drop(cr);
    surface.flush();

    let stride = surface.stride() as usize;
    let data = surface
        .data()
        .map_err(|e| tracing::error!("Failed to get surface data: {:?}", e))
        .ok()?;

    let bytes = glib::Bytes::from(&data[..]);

    Some(gdk::MemoryTexture::new(
        width,
        height,
        gdk::MemoryFormat::B8g8r8a8,
        &bytes,
        stride,
    ))
}

pub fn generate_thumbnail(
    file: &gio::File,
    rotation: i32,
    password: Option<&str>,
    max_dim: f64,
) -> Result<ThumbnailResult, PreviewError> {
    generate_page_thumbnail(file, 0, rotation, password, max_dim)
}

pub fn generate_page_thumbnail(
    file: &gio::File,
    page_index: i32,
    rotation: i32,
    password: Option<&str>,
    max_dim: f64,
) -> Result<ThumbnailResult, PreviewError> {
    generate_watermark_thumbnail(file, page_index, rotation, password, max_dim, None, 1.0)
}

pub fn generate_watermark_thumbnail(
    file: &gio::File,
    page_index: i32,
    rotation: i32,
    password: Option<&str>,
    max_dim: f64,
    watermark_file: Option<&gio::File>,
    opacity: f64,
) -> Result<ThumbnailResult, PreviewError> {
    let doc = match poppler::Document::from_gfile(file, password, gio::Cancellable::NONE) {
        Ok(d) => d,
        Err(e) => {
            if e.matches(poppler::Error::Encrypted) {
                return Err(PreviewError::Encrypted);
            }
            tracing::error!("Failed to open poppler doc: {:?}", e);
            return Err(PreviewError::Other);
        }
    };

    let Some(page) = doc.page(page_index) else {
        tracing::error!("Failed to get page {}", page_index);
        return Ok(ThumbnailResult {
            texture: None,
            original_dimensions: None,
            page_count: doc.n_pages(),
        });
    };

    let (orig_width, orig_height) = page.size();
    let (width, height) = if rotation % 180 != 0 {
        (orig_height, orig_width)
    } else {
        (orig_width, orig_height)
    };

    let scale = max_dim / width.max(height);
    let scaled_width = (width * scale) as i32;
    let scaled_height = (height * scale) as i32;

    let texture = render_to_texture(scaled_width, scaled_height, |cr| {
        let _ = cr.save();
        cr.scale(scale, scale);
        let angle = (rotation as f64) * std::f64::consts::PI / 180.0;

        match rotation.rem_euclid(360) {
            90 => {
                cr.translate(orig_height, 0.0);
                cr.rotate(angle);
            }
            180 => {
                cr.translate(orig_width, orig_height);
                cr.rotate(angle);
            }
            270 => {
                cr.translate(0.0, orig_width);
                cr.rotate(angle);
            }
            _ => {}
        }

        page.render(cr);
        let _ = cr.restore();

        if let Some(wm_file) = watermark_file {
            render_watermark_on_preview(cr, scale, width, height, wm_file, opacity);
        }
    });

    Ok(ThumbnailResult {
        texture,
        original_dimensions: Some((orig_width, orig_height)),
        page_count: doc.n_pages(),
    })
}

fn render_watermark_on_preview(
    cr: &cairo::Context,
    scale: f64,
    width: f64,
    height: f64,
    wm_file: &gio::File,
    opacity: f64,
) {
    if let Some(path) = wm_file.path()
        && let Ok(img) = image::open(&path)
    {
        let rgba = img.to_rgba8();
        let wm_w = rgba.width() as f64;
        let wm_h = rgba.height() as f64;
        let wm_scale = (width / wm_w).min(height / wm_h).min(1.0);
        let scaled_wm_w = wm_w * wm_scale;
        let scaled_wm_h = wm_h * wm_scale;
        let x = (width - scaled_wm_w) / 2.0;
        let y = (height - scaled_wm_h) / 2.0;

        if let Ok(mut surface) = cairo::ImageSurface::create(
            cairo::Format::ARgb32,
            rgba.width() as i32,
            rgba.height() as i32,
        ) {
            let stride = surface.stride() as usize;
            if let Ok(mut data) = surface.data() {
                for (iy, row) in rgba.rows().enumerate() {
                    for (ix, pixel) in row.enumerate() {
                        let r = pixel[0] as u32;
                        let g = pixel[1] as u32;
                        let b = pixel[2] as u32;
                        let a = pixel[3] as u32;

                        let pr = (r * a) / 255;
                        let pg = (g * a) / 255;
                        let pb = (b * a) / 255;

                        let offset = iy * stride + ix * 4;
                        data[offset] = pb as u8;
                        data[offset + 1] = pg as u8;
                        data[offset + 2] = pr as u8;
                        data[offset + 3] = a as u8;
                    }
                }
            }
            surface.mark_dirty();

            let _ = cr.save();
            cr.scale(scale, scale);
            cr.rectangle(0.0, 0.0, width, height);
            cr.clip();

            cr.push_group();
            cr.translate(x, y);
            cr.scale(wm_scale, wm_scale);
            let _ = cr.set_source_surface(&surface, 0.0, 0.0);
            let _ = cr.paint();
            let _ = cr.pop_group_to_source();
            let _ = cr.paint_with_alpha(opacity);
            let _ = cr.restore();
        }
    }
}

pub fn generate_blank_thumbnail(
    orig_width: f64,
    orig_height: f64,
    rotation: i32,
    max_dim: f64,
) -> Result<ThumbnailResult, PreviewError> {
    let (width, height) = if rotation % 180 != 0 {
        (orig_height, orig_width)
    } else {
        (orig_width, orig_height)
    };

    let scale = max_dim / width.max(height);
    let scaled_width = (width * scale) as i32;
    let scaled_height = (height * scale) as i32;

    let texture = render_to_texture(scaled_width, scaled_height, |_| {});

    Ok(ThumbnailResult {
        texture,
        original_dimensions: Some((orig_width, orig_height)),
        page_count: 1,
    })
}

fn add_border(cr: &cairo::Context, width: i32, height: i32) {
    cr.identity_matrix();
    cr.set_source_rgb(0.8, 0.8, 0.8);
    cr.set_line_width(2.0);
    cr.rectangle(0.0, 0.0, width as f64, height as f64);
    let _ = cr.stroke();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_utils::create_test_doc;
    use lopdf::{Dictionary, Object, StringFormat};
    use relm4::gtk::prelude::TextureExt;

    fn init_gtk() {
        let _ = relm4::gtk::init();
    }

    #[test]
    fn test_prv_01_rotation_dimension_swapping() {
        init_gtk();
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("landscape.pdf");

        let mut doc = create_test_doc(1, 800.0, 600.0);
        doc.save(&file_path).unwrap();

        let gio_file = gio::File::for_path(&file_path);
        let result = generate_thumbnail(&gio_file, 90, None, 400.0)
            .expect("Expected thumbnail generation to succeed");

        assert_eq!(result.page_count, 1);
        assert_eq!(result.original_dimensions, Some((800.0, 600.0)));

        let texture = result.texture.expect("Expected a valid texture");
        assert_eq!(texture.width(), 300);
        assert_eq!(texture.height(), 400);
    }

    #[test]
    fn test_prv_02_encrypted_document_graceful_rejection() {
        init_gtk();
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("encrypted.pdf");

        let mut doc = create_test_doc(1, 595.0, 842.0);
        let encrypt_id = (doc.max_id + 1, 0);
        doc.max_id += 1;

        let mut encrypt_dict = Dictionary::new();
        encrypt_dict.set("Filter", "Standard");
        encrypt_dict.set("V", 2);
        encrypt_dict.set("R", 3);
        encrypt_dict.set("O", Object::String(vec![1_u8; 32], StringFormat::Literal));
        encrypt_dict.set("U", Object::String(vec![2_u8; 32], StringFormat::Literal));
        encrypt_dict.set("P", -4_i64);
        doc.objects.insert(encrypt_id, Object::Dictionary(encrypt_dict));
        doc.trailer.set("Encrypt", encrypt_id);

        doc.save(&file_path).unwrap();

        let gio_file = gio::File::for_path(&file_path);
        let result = generate_thumbnail(&gio_file, 0, None, 200.0);

        match result {
            Err(PreviewError::Encrypted) => {}
            other => panic!("Expected Err(PreviewError::Encrypted), got {:?}", other),
        }
    }

    #[test]
    fn test_prv_03_blank_thumbnail_synthesis() {
        init_gtk();
        let result = generate_blank_thumbnail(595.0, 842.0, 0, 200.0)
            .expect("Expected blank thumbnail generation to succeed");

        assert_eq!(result.page_count, 1);
        assert_eq!(result.original_dimensions, Some((595.0, 842.0)));

        let texture = result.texture.expect("Expected a valid texture");
        assert_eq!(texture.width(), 141);
        assert_eq!(texture.height(), 200);
    }
}

