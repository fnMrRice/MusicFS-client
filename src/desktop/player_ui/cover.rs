use crate::task::Task;
use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use musicfs_server_adapter::ServerSession;
use std::io::Cursor;
use std::sync::Arc;
use tokio::runtime::Handle;

#[derive(Default)]
pub struct Cover {
    key: Option<(u64, String)>,
    pending: Option<Task<(ColorImage, Arc<[u8]>)>>,
    pub texture: Option<TextureHandle>,
    png: Option<Arc<[u8]>>,
}

impl Cover {
    pub fn png_for(&self, instance: u64, id: Option<&str>) -> Option<Arc<[u8]>> {
        let (owner, object) = self.key.as_ref()?;
        if *owner == instance && Some(object.as_str()) == id {
            self.png.clone()
        } else {
            None
        }
    }

    pub fn update(
        &mut self,
        ctx: &Context,
        runtime: &Handle,
        session: &ServerSession,
        instance: u64,
        id: Option<&str>,
    ) {
        let key = id.map(|id| (instance, id.to_owned()));
        if self.key != key {
            self.key = key;
            self.texture = None;
            self.png = None;
            self.pending = id.map(|id| {
                let session = session.clone();
                let id = id.to_owned();
                Task::spawn(runtime, ctx, async move {
                    let bytes = session.cover_bytes(&id).await?;
                    tokio::task::spawn_blocking(move || playing_artwork(bytes))
                        .await
                        .map_err(|_| "Artwork decoding interrupted.")?
                })
            });
        }
        if let Some(result) = self.pending.as_mut().and_then(Task::poll) {
            self.pending = None;
            if let Ok((image, png)) = result {
                self.png = Some(png);
                self.texture =
                    Some(ctx.load_texture("playing-track-cover", image, TextureOptions::LINEAR));
            }
        }
    }
}

pub(crate) async fn read_thumbnail(
    session: ServerSession,
    id: String,
) -> Result<ColorImage, String> {
    let bytes = session.cover_bytes(&id).await?;
    tokio::task::spawn_blocking(move || thumbnail(bytes).map(|img| color_image(&img)))
        .await
        .map_err(|_| "Artwork decoding interrupted.")?
}

fn playing_artwork(bytes: Vec<u8>) -> Result<(ColorImage, Arc<[u8]>), String> {
    use image::ImageEncoder;
    let image = thumbnail(bytes)?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &image,
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok((color_image(&image), Arc::from(png)))
}

fn thumbnail(bytes: Vec<u8>) -> Result<image::RgbaImage, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader
        .decode()
        .map_err(|e| e.to_string())?
        .thumbnail(160, 160)
        .to_rgba8())
}

fn color_image(image: &image::RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    )
}
