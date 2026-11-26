use super::{Artwork, Category, Services, Task};
use crate::{browse::resource::Resource, i18n::tr};
use eframe::egui;

#[cfg(test)]
#[path = "../../../../tests/native/album_cover.rs"]
mod tests;

#[derive(Default)]
pub(super) struct AlbumCover {
    album: String,
    metadata: Resource<Option<String>>,
    updating: Option<(String, Task<()>)>,
    outcome: Option<(String, Result<(), String>)>,
}

impl AlbumCover {
    pub(super) fn poll(&mut self, artwork: &mut Artwork) {
        let Some((path, task)) = self.updating.as_mut() else {
            return;
        };
        let Some(result) = task.poll() else { return };
        if result.is_ok() {
            artwork.invalidate(path);
        }
        self.outcome = Some((path.clone(), result));
        self.updating = None;
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        artwork: &mut Artwork,
        id: &str,
        known: Option<&Category>,
    ) {
        if id.is_empty() {
            return;
        }
        if self.album != id {
            self.album = id.to_owned();
            self.metadata = Default::default();
        }
        self.metadata.poll();
        // 从曲目署名直接进入时，只为当前专辑读取一次封面资料。
        if known.is_none()
            && (!self.metadata.started || self.metadata.status(ui, tr("正在读取专辑封面")))
        {
            let server = svc.server.clone();
            let album = id.to_owned();
            self.metadata.start(ui.ctx(), svc.runtime, async move {
                server.album_cover_path(&album).await
            });
        }
        let path = match known {
            Some(item) => item.cover_path.as_deref(),
            None => self.metadata.value.as_ref().and_then(|p| p.as_deref()),
        };
        let Some(path) = path else { return };
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(40.), egui::Sense::hover());
        artwork.paint(ui, rect, svc, Some(path));
        if !path.starts_with("/api/v1/library/albums/") {
            return;
        }
        if ui
            .add_enabled(self.updating.is_none(), egui::Button::new(tr("更新封面")))
            .clicked()
        {
            let server = svc.server.clone();
            let album = id.to_owned();
            self.outcome = None;
            self.updating = Some((
                path.to_owned(),
                Task::spawn(svc.runtime, ui.ctx(), async move {
                    server.refresh_album_cover(&album).await
                }),
            ));
        }
        if self
            .updating
            .as_ref()
            .is_some_and(|(target, _)| target == path)
        {
            ui.spinner();
        }
        if let Some((_, result)) = self.outcome.as_ref().filter(|(target, _)| target == path) {
            match result {
                Ok(()) => ui.weak(tr("封面已更新")),
                Err(error) => ui.colored_label(ui.visuals().error_fg_color, error),
            };
        }
    }
}
