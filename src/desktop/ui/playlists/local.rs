use crate::{
    i18n::tr,
    icons,
    ui::{
        card_grid,
        data::{Pages, Services},
        style,
    },
};
use eframe::egui::{self, FontId, Rect, RichText, Stroke, StrokeKind};
use musicfs_server_adapter::Playlist;

mod editor;
mod entries;
mod rules;

#[cfg(test)]
#[path = "../../../../tests/native/local_playlist_management.rs"]
mod tests;

use editor::{Editor, Outcome};

#[derive(Default)]
pub(crate) struct LocalPlaylists {
    editor: Option<Editor>,
}

pub(crate) enum LocalAction {
    Open(String),
    More,
    Refresh,
}

impl LocalPlaylists {
    pub(crate) fn list(
        &mut self,
        ui: &mut egui::Ui,
        lists: &Pages<Playlist>,
    ) -> Option<LocalAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            if style::primary(ui, tr("新建歌单"), true).clicked() {
                self.editor = Some(Editor::new(None));
            }
            if !lists.items.is_empty() {
                let count = lists.items.len().to_string();
                let label = if lists.next.is_some() {
                    tr("已加载 {0} 个歌单")
                } else {
                    tr("{0} 个歌单")
                };
                ui.weak(label.replace("{0}", &count));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, "refresh", tr("刷新"), 34., 18.).clicked() {
                    action = Some(LocalAction::Refresh);
                }
            });
        });
        ui.add_space(6.);
        egui::ScrollArea::vertical()
            .id_salt("personal-playlists")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if lists.items.is_empty()
                    && lists.error.is_none()
                    && !lists.busy()
                    && lists.started
                    && lists.next.is_none()
                {
                    let dark = ui.visuals().dark_mode;
                    egui::Frame::new()
                        .fill(style::surface(dark))
                        .stroke(Stroke::new(1_f32, style::line(dark)))
                        .corner_radius(10)
                        .inner_margin(24)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical_centered(|ui| {
                                ui.add_space(12.);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::Vec2::splat(48.),
                                    egui::Sense::hover(),
                                );
                                mark(ui, rect);
                                ui.add_space(6.);
                                ui.label(
                                    RichText::new(tr("还没有个人歌单")).font(style::bold(ui, 18.)),
                                );
                                ui.weak(tr("新建歌单后，可从曲目列表添加歌曲。"));
                                ui.add_space(12.);
                            });
                        });
                }
                if let Some(id) = cards(ui, &lists.items) {
                    action = Some(LocalAction::Open(id));
                }
                if let Some(error) = &lists.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if ui.button(tr("重试")).clicked() {
                        action = Some(if lists.items.is_empty() || lists.next.is_none() {
                            LocalAction::Refresh
                        } else {
                            LocalAction::More
                        });
                    }
                } else if lists.busy() || !lists.started {
                    ui.spinner();
                } else if action.is_none()
                    && lists.can_load_more()
                    && crate::ui::page_end_visible(ui)
                {
                    action = Some(LocalAction::More);
                }
            });
        action
    }

    pub(crate) fn detail(&mut self, ui: &mut egui::Ui, playlist: &Playlist) {
        if ui.button(tr("编辑歌单")).clicked() {
            self.editor = Some(Editor::new(Some(playlist.id.clone())));
        }
    }

    pub(crate) fn dialogs(
        &mut self,
        ctx: &egui::Context,
        svc: Services<'_>,
        lists: &Pages<Playlist>,
    ) -> bool {
        let Some(editor) = &mut self.editor else {
            return false;
        };
        match editor.show(ctx, svc, lists) {
            Outcome::Stay => false,
            Outcome::Close => {
                self.editor = None;
                false
            }
            Outcome::Saved => {
                self.editor = None;
                true
            }
        }
    }
}

fn mark(ui: &egui::Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 9, style::accent_soft(ui));
    icons::paint(
        ui,
        "queue",
        rect.shrink(rect.width() * 0.24),
        style::accent(ui),
    );
}

fn cards(ui: &mut egui::Ui, items: &[Playlist]) -> Option<String> {
    let width = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    let cols = ((width + gap) / (254. + gap)).floor().clamp(1., 4.) as usize;
    let cell = egui::vec2(((width + gap) / cols as f32 - gap).max(1.), 82.);
    let mut chosen = None;
    card_grid(ui, items.len(), cols, cell, |ui, index, rect, response| {
        let playlist = &items[index];
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &playlist.name)
        });
        if response.clicked() {
            chosen = Some(playlist.id.clone());
        }
        if !ui.is_rect_visible(rect) {
            return;
        }
        let dark = ui.visuals().dark_mode;
        let fill = if response.hovered() || response.has_focus() {
            style::hovered(dark)
        } else {
            style::surface(dark)
        };
        let edge = if response.has_focus() {
            style::accent(ui)
        } else {
            style::line(dark)
        };
        ui.painter()
            .rect(rect, 9, fill, Stroke::new(1_f32, edge), StrokeKind::Inside);
        let inside = rect.shrink(13.);
        let tile = Rect::from_min_size(inside.left_top(), egui::Vec2::splat(inside.height()));
        mark(ui, tile);
        let x = tile.right() + 12.;
        let color = style::text(dark);
        let mut title = egui::text::LayoutJob::simple(
            playlist.name.clone(),
            style::bold(ui, 15.),
            color,
            (inside.right() - x).max(1.),
        );
        title.wrap.max_rows = 1;
        let title = ui.fonts(|f| f.layout_job(title));
        let painter = ui.painter_at(inside);
        painter.galley(egui::pos2(x, inside.top() + 6.), title, color);
        let status = if playlist.snapshot_revision == 0 {
            tr("尚未发布")
        } else if playlist.stale {
            tr("有待发布的更改")
        } else {
            tr("已发布")
        };
        painter.text(
            egui::pos2(x, inside.bottom() - 6.),
            egui::Align2::LEFT_BOTTOM,
            status,
            FontId::proportional(12.),
            style::muted(dark),
        );
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(&playlist.name);
    });
    chosen
}
