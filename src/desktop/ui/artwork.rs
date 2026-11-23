use super::Services;
use crate::{player_ui::cover::read_thumbnail, task::Task};
use eframe::egui::{self, ColorImage, TextureHandle};
use std::collections::HashMap;

enum ImageState {
    Loading(Task<ColorImage>),
    Ready(TextureHandle),
    Failed,
}
#[derive(Default)]
pub struct Artwork {
    items: HashMap<String, (u64, ImageState)>,
    counter: u64,
}

impl Artwork {
    pub fn poll(&mut self, ctx: &egui::Context) {
        for (id, (_, entry)) in &mut self.items {
            if let ImageState::Loading(pending) = entry {
                if let Some(result) = pending.poll() {
                    *entry = match result {
                        Ok(image) => ImageState::Ready(ctx.load_texture(
                            id,
                            image,
                            egui::TextureOptions::LINEAR,
                        )),
                        Err(_) => ImageState::Failed,
                    }
                }
            }
        }
    }
    pub fn paint(&mut self, ui: &egui::Ui, rect: egui::Rect, svc: Services<'_>, id: Option<&str>) {
        if let Some(id) = id {
            self.counter += 1;
            if !self.items.contains_key(id)
                && self
                    .items
                    .values()
                    .filter(|(_, v)| matches!(v, ImageState::Loading(_)))
                    .count()
                    < 4
            {
                if self.items.len() >= 128 {
                    if let Some(key) = self
                        .items
                        .iter()
                        .filter(|(_, (_, s))| !matches!(s, ImageState::Loading(_)))
                        .min_by_key(|(_, (stamp, _))| *stamp)
                        .map(|(k, _)| k.clone())
                    {
                        self.items.remove(&key);
                    }
                }
                let pending = Task::spawn(
                    svc.runtime,
                    ui.ctx(),
                    read_thumbnail(svc.server.clone(), id.to_owned()),
                );
                self.items
                    .insert(id.to_owned(), (self.counter, ImageState::Loading(pending)));
            }
            if let Some((stamp, state)) = self.items.get_mut(id) {
                *stamp = self.counter;
                if let ImageState::Ready(texture) = state {
                    let size = texture.size_vec2();
                    let side = size.x.min(size.y);
                    let uv = egui::Rect::from_center_size(
                        egui::pos2(0.5, 0.5),
                        egui::vec2(side / size.x, side / size.y),
                    );
                    egui::Image::new(&*texture)
                        .uv(uv)
                        .corner_radius(6)
                        .paint_at(ui, rect);
                    return;
                }
            }
        }
        ui.painter()
            .rect_filled(rect, 6, egui::Color32::from_rgb(35, 46, 56));
        crate::icons::paint(
            ui,
            "music",
            egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(22.)),
            egui::Color32::from_rgb(151, 165, 183),
        );
    }
}
