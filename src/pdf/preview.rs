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

#[derive(Debug)]
pub struct ThumbnailResult {
    pub texture: Option<gdk::MemoryTexture>,
    pub original_dimensions: Option<(f64, f64)>,
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
        return Ok(ThumbnailResult {
            texture: None,
            original_dimensions: None,
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
    });

    Ok(ThumbnailResult {
        texture,
        original_dimensions: Some((orig_width, orig_height)),
    })
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
    })
}

fn add_border(cr: &cairo::Context, width: i32, height: i32) {
    cr.identity_matrix();
    cr.set_source_rgb(0.8, 0.8, 0.8);
    cr.set_line_width(2.0);
    cr.rectangle(0.0, 0.0, width as f64, height as f64);
    let _ = cr.stroke();
}
