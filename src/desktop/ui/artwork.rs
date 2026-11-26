use super::Services;
use crate::{player_ui::cover::read_thumbnail, task::Task};
use eframe::egui::{self, ColorImage, TextureHandle};
use std::collections::HashMap;

const MAX_ENTRIES: usize = 128;
const MAX_PENDING: usize = 4;

struct Pending {
    id: String,
    last_used: u64,
    task: Task<ColorImage>,
    retry_if_failed: bool,
}
#[derive(Default)]
pub struct Artwork {
    // 缓存失败以防止每帧发请求；用户刷新分类时只清除失败项。
    items: HashMap<String, (u64, Option<TextureHandle>)>,
    pending: Vec<Pending>,
    counter: u64,
}

impl Artwork {
    pub fn retry_failed(&mut self) {
        self.items.retain(|_, (_, texture)| texture.is_some());
        for pending in &mut self.pending {
            pending.retry_if_failed = true;
        }
    }

    pub fn invalidate(&mut self, id: &str) {
        self.items.remove(id);
        self.pending.retain(|item| item.id != id);
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        self.pending.retain_mut(|pending| {
            let Some(result) = pending.task.poll() else {
                return true;
            };
            if result.is_err() && pending.retry_if_failed {
                return false;
            }
            let texture = result
                .ok()
                .map(|image| ctx.load_texture(&pending.id, image, egui::TextureOptions::LINEAR));
            self.items.insert(
                std::mem::take(&mut pending.id),
                (pending.last_used, texture),
            );
            false
        });
    }
    fn touch(&mut self, id: &str, start: impl FnOnce() -> Task<ColorImage>) {
        self.counter += 1;
        if let Some((stamp, _)) = self.items.get_mut(id) {
            *stamp = self.counter;
            return;
        }
        if let Some(entry) = self.pending.iter_mut().find(|entry| entry.id == id) {
            entry.last_used = self.counter;
            return;
        }
        if self.pending.len() == MAX_PENDING {
            return;
        }
        if self.items.len() + self.pending.len() >= MAX_ENTRIES {
            if let Some(oldest) = self
                .items
                .iter()
                .min_by_key(|(_, (stamp, _))| *stamp)
                .map(|(id, _)| id.clone())
            {
                self.items.remove(&oldest);
            }
        }
        self.pending.push(Pending {
            id: id.to_owned(),
            last_used: self.counter,
            task: start(),
            retry_if_failed: false,
        });
    }
    pub fn paint(&mut self, ui: &egui::Ui, rect: egui::Rect, svc: Services<'_>, id: Option<&str>) {
        if let Some(id) = id {
            self.touch(id, || {
                Task::spawn(
                    svc.runtime,
                    ui.ctx(),
                    read_thumbnail(svc.server.clone(), id.to_owned()),
                )
            });
            if let Some((_, Some(texture))) = self.items.get(id) {
                let size = texture.size_vec2();
                let side = size.x.min(size.y);
                let uv = egui::Rect::from_center_size(
                    egui::pos2(0.5, 0.5),
                    egui::vec2(side / size.x, side / size.y),
                );
                egui::Image::new(texture)
                    .uv(uv)
                    .corner_radius(6)
                    .paint_at(ui, rect);
                return;
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

#[cfg(test)]
#[path = "../../../tests/native/artwork.rs"]
mod tests;
