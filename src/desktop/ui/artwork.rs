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
    pub fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>, id: Option<&str>) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(32., 32.), egui::Sense::hover());
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
                    egui::Image::new(&*texture)
                        .corner_radius(4)
                        .paint_at(ui, rect);
                    return;
                }
            }
        }
        let painter = ui.painter();
        painter.rect_filled(rect, 4, ui.visuals().widgets.inactive.bg_fill);
        painter.circle_stroke(
            rect.center(),
            10.,
            egui::Stroke::new(1.5, ui.visuals().weak_text_color()),
        );
        painter.circle_filled(rect.center(), 2.5, ui.visuals().weak_text_color());
    }
}
