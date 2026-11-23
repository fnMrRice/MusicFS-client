use super::{egui, Load, Services};
#[derive(Default)]
pub(super) struct Avatar {
    url: Option<String>,
    request: Load<egui::ColorImage>,
    texture: Option<egui::TextureHandle>,
}
impl Avatar {
    pub fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>, url: Option<&str>) {
        if self.url.as_deref() != url {
            *self = Self {
                url: url.map(str::to_owned),
                ..Default::default()
            };
            if let Some(url) = &self.url {
                let (s, url) = (svc.server.clone(), url.clone());
                self.request.start(ui, svc, async move {
                    let bytes = s.platform_avatar(&url).await?;
                    tokio::task::spawn_blocking(move || {
                        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                            .with_guessed_format()
                            .map_err(|_| "Invalid avatar image.")?;
                        let mut limits = image::Limits::default();
                        limits.max_image_width = Some(4096);
                        limits.max_image_height = Some(4096);
                        limits.max_alloc = Some(32 * 1024 * 1024);
                        reader.limits(limits);
                        let image = reader
                            .decode()
                            .map_err(|_| "Unsupported avatar image.")?
                            .thumbnail(96, 96)
                            .to_rgba8();
                        Ok(egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        ))
                    })
                    .await
                    .map_err(|_| "Interrupted avatar decoder.")?
                });
            }
        }
        if self.request.poll() {
            self.texture = self.request.value.take().map(|image| {
                ui.ctx().load_texture(
                    "platform-account-avatar",
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
        }
        if let Some(texture) = &self.texture {
            ui.image((texture.id(), egui::vec2(40., 40.)));
        }
    }
}
