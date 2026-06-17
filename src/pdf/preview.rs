use relm4::gtk::cairo;
use relm4::gtk::{gdk, gio, glib};
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

pub fn generate_thumbnail(
    file: &gio::File,
    rotation: i32,
    password: Option<&str>,
) -> Result<Option<gdk::MemoryTexture>, PreviewError> {
    let doc = match poppler::Document::from_gfile(file, password, gio::Cancellable::NONE) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to open poppler doc: {:?}", e);
            if e.matches(poppler::Error::Encrypted) {
                return Err(PreviewError::Encrypted);
            }
            return Err(PreviewError::Other);
        }
    };

    let Some(page) = doc.page(0) else {
        tracing::error!("Failed to get first page");
        return Ok(None);
    };

    let (orig_width, orig_height) = page.size();
    let (width, height) = if rotation % 180 != 0 {
        (orig_height, orig_width)
    } else {
        (orig_width, orig_height)
    };

    let scale = 150.0 / width.max(height);
    let scaled_width = (width * scale) as i32;
    let scaled_height = (height * scale) as i32;

    let Some(mut surface) =
        cairo::ImageSurface::create(cairo::Format::ARgb32, scaled_width, scaled_height)
            .map_err(|e| tracing::error!("Failed to create surface: {:?}", e))
            .ok()
    else {
        return Ok(None);
    };
    let Some(cr) = cairo::Context::new(&surface)
        .map_err(|e| tracing::error!("Failed to create context: {:?}", e))
        .ok()
    else {
        return Ok(None);
    };

    cr.set_source_rgb(1.0, 1.0, 1.0);
    if let Err(e) = cr.paint() {
        tracing::error!("Failed to paint background: {:?}", e);
        return Ok(None);
    }

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

    page.render(&cr);
    drop(cr);
    surface.flush();

    let stride = surface.stride() as usize;
    let Some(data) = surface
        .data()
        .map_err(|e| tracing::error!("Failed to get surface data: {:?}", e))
        .ok()
    else {
        return Ok(None);
    };

    let bytes = glib::Bytes::from(&data[..]);

    Ok(Some(gdk::MemoryTexture::new(
        scaled_width,
        scaled_height,
        gdk::MemoryFormat::B8g8r8a8,
        &bytes,
        stride,
    )))
}
