use super::super::actions::Actions;
use super::*;
use crate::icons;
use musicfs_server_adapter::Playlist;

const SORT_WIDTH: f32 = 108.;

#[cfg(test)]
#[path = "../../../../tests/native/catalog_toolbar.rs"]
mod tests;

// 表格内容可能宽于视口，工具栏只使用当前可见的宽度。
fn available_width(ui: &egui::Ui) -> f32 {
    ui.available_rect_before_wrap()
        .intersect(ui.clip_rect())
        .intersect(ui.ctx().screen_rect())
        .width()
        .max(0.)
}

impl Catalog {
    pub(super) fn category_tabs(&mut self, ui: &mut egui::Ui) {
        let span = ui.max_rect().intersect(ui.clip_rect());
        let mut underline = None;
        let mut selected = None;
        let row = egui::ScrollArea::horizontal()
            .id_salt("catalog-tabs")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.;
                    ui.spacing_mut().button_padding = egui::vec2(18., 10.);
                    ui.spacing_mut().interact_size.y = 46.;
                    for tab in [
                        CategoryTab::All,
                        CategoryTab::Artist,
                        CategoryTab::Album,
                        CategoryTab::Genre,
                        CategoryTab::Folder,
                    ] {
                        let active = self.tabs == tab;
                        let text = RichText::new(tab.label()).size(16.);
                        let text = if active {
                            text.font(style::bold(ui, 16.))
                        } else {
                            text
                        };
                        let r = style::selectable(ui, active, text);
                        if r.clicked() && !active {
                            selected = Some(tab);
                        }
                        if active {
                            let mut rect = r.rect;
                            rect.min.y = rect.bottom() - 3.;
                            underline = Some(rect);
                        }
                    }
                });
            });
        let y = underline.map_or(row.inner_rect.bottom(), |rect| rect.bottom());
        let rule =
            egui::Rect::from_min_max(egui::pos2(span.left(), y - 1.), egui::pos2(span.right(), y));
        ui.painter()
            .rect_filled(rule, 0, style::line(ui.visuals().dark_mode));
        if let Some(rect) = underline {
            ui.painter()
                .rect_filled(rect.intersect(span), 2, style::accent(ui));
        }
        ui.add_space(10.);
        if let Some(tab) = selected {
            self.list_query.clear();
            self.list_draft.clear();
            self.tabs = tab;
            self.reset_categories();
        }
    }

    pub(super) fn list_search(
        &mut self,
        ui: &mut egui::Ui,
        refresh_label: &str,
        leading: impl FnOnce(&mut Self, &mut egui::Ui),
    ) -> bool {
        let buttons = 36. + ui.spacing().item_spacing.x + self.clear_search_width(ui);
        if available_width(ui) < 90. + buttons && ui.layout().main_wrap() {
            ui.end_row();
        }
        let available = available_width(ui);
        let width = (available - buttons).clamp(0., 190.);
        ui.allocate_ui_with_layout(
            egui::vec2(available, 36.),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let refresh = ui.allocate_ui_with_layout(
                    egui::vec2(width + buttons, 36.),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        self.search_field(ui, width);
                        icons::button(ui, "refresh", refresh_label, 36., 18.).clicked()
                    },
                );
                leading(self, ui);
                refresh.inner
            },
        )
        .inner
    }

    fn clear_search_width(&self, ui: &egui::Ui) -> f32 {
        if self.list_draft.is_empty() && self.list_query.is_empty() {
            0.
        } else {
            28. + ui.spacing().item_spacing.x
        }
    }

    fn search_field(&mut self, ui: &mut egui::Ui, width: f32) {
        let focus = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
        let has_text = !self.list_draft.is_empty() || !self.list_query.is_empty();
        let field = ui.add_sized(
            [width, 36.],
            egui::TextEdit::singleline(&mut self.list_draft)
                .hint_text(tr("搜索当前列表"))
                .margin(egui::Margin::symmetric(12, 4))
                .char_limit(256)
                .vertical_align(egui::Align::Center),
        );
        if focus {
            field.request_focus();
        }
        let submit = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let clear = has_text && icons::button(ui, "close", tr("清除"), 28., 14.).clicked();
        if clear {
            self.list_draft.clear();
        }
        if (submit || clear || self.list_draft.is_empty())
            && self.list_query != self.list_draft.trim()
        {
            self.list_query = self.list_draft.trim().into();
            self.categories = Default::default();
            self.reset_tracks();
        }
    }

    fn all_track_actions(&mut self, ui: &mut egui::Ui, svc: Services<'_>) {
        style::menu_button(ui, tr("更多"), |ui| {
            if style::button(ui, tr("新建曲目")).clicked() {
                self.creation = Some(Default::default());
                ui.close_menu();
            }
            self.matching.buttons(ui, svc);
        });
    }

    fn creation_status(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> bool {
        self.providers.status(ui);
        if let Some(saved) = self
            .creation
            .as_mut()
            .and_then(|dialog| dialog.show(ui, svc))
        {
            self.creation = None;
            if saved {
                self.feedback = Some(tr("已新建曲目，可在搜索中查看。").into());
                return true;
            }
        }
        false
    }

    pub fn table_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        actions: &mut Actions,
        query: &TrackQuery,
        lists: &[Playlist],
    ) -> bool {
        let all = matches!(query, TrackQuery::AvailableSources { .. });
        let library = all || matches!(query, TrackQuery::Category { .. });
        let search = matches!(query, TrackQuery::Library(_));
        let matching = library || search;
        let mut refresh = false;
        self.poll_queue(svc, actions);
        let album = match query {
            TrackQuery::Category {
                tab: CategoryTab::Album,
                id,
                ..
            } => Some(id),
            _ => None,
        };
        let row = |ui: &mut egui::Ui| {
            ui.spacing_mut().interact_size.y = 38.;
            if all {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.spacing_mut().item_spacing.x = 6.;
                ui.spacing_mut().button_padding.x = 8.;
            }
            let checked = self.matching.selected_tracks();
            if checked.is_empty() {
                self.queue_buttons(ui, svc, actions);
                if all {
                    self.all_track_actions(ui, svc);
                }
            } else {
                ui.strong(tr("已选择 {0} 项").replace("{0}", &checked.len().to_string()));
                if style::add_button(
                    ui,
                    checked.iter().any(|t| t.playable_source_id().is_some()),
                    egui::Button::new(tr("加入队列")),
                )
                .clicked()
                {
                    match super::queue::enqueue(
                        svc,
                        checked.iter().filter_map(queue_track).collect(),
                    ) {
                        Ok(message) => self.feedback = Some(message),
                        Err(error) => actions.notice = Some((error, true)),
                    }
                }
                if style::add_button(
                    ui,
                    !actions.busy() && checked.iter().all(|t| !t.unresolved),
                    egui::Button::new(tr("添加到歌单")),
                )
                .clicked()
                {
                    actions.open_many(checked, lists);
                }
                if style::add_button(
                    ui,
                    actions.can_favorite_many(checked),
                    egui::Button::new(tr("收藏")),
                )
                .clicked()
                {
                    actions.favorite_many(ui, svc, checked);
                }
                if let Some(id) = album {
                    refresh |= self.album_merge.show(ui, svc, id, checked);
                }
                if all && self.matching.selected_tracks().len() == 2 {
                    style::menu_button(ui, tr("曲目关联"), |ui| {
                        self.matching.selection_buttons(ui, true)
                    });
                } else if matching {
                    self.matching.selection_buttons(ui, album.is_none());
                }
                if style::button(ui, tr("清空选择")).clicked() {
                    self.matching.clear_selection();
                }
            }
            if !library && icons::button(ui, "refresh", tr("刷新列表"), 32., 17.).clicked() {
                self.artwork = Default::default();
                actions.favorites.refresh();
                refresh = true;
            }
            if self.tracks.busy() || self.complete.is_some() {
                ui.spinner();
            }
            if search {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    style::menu_button(ui, tr("更多操作"), |ui| self.matching.buttons(ui, svc));
                });
                return;
            }
            if query.is_playlist() {
                if self.playlist.tools.show(ui) {
                    self.playlist.rebuild(&mut self.tracks.items);
                    self.matching.clear_selection();
                    self.versions.clear_expanded();
                    self.complete = None;
                    ui.ctx().request_repaint();
                }
                return;
            }
            let sort = |ui: &mut egui::Ui| {
                providers::disabled_select(
                    ui,
                    "track-sort",
                    tr("默认顺序"),
                    SORT_WIDTH,
                    16.,
                    tr("等待服务端提供完整查询排序能力"),
                );
            };
            if library {
                let before = self.list_query.clone();
                if self.list_search(ui, tr("刷新列表"), |_, ui| sort(ui)) {
                    self.providers.refresh();
                    self.artwork = Default::default();
                    actions.favorites.refresh();
                    refresh = true;
                }
                refresh |= self.list_query != before;
            } else {
                sort(ui);
            }
        };
        if !all && !search {
            ui.add_space(17.);
        }
        ui.horizontal_wrapped(row);
        if all {
            refresh |= self.creation_status(ui, svc);
        }
        if matching {
            refresh |= self
                .matching
                .update(ui, svc, &|id| self.providers.label(id).into());
        }
        ui.add_space(10.);
        if let Some(message) = self.feedback.take() {
            actions.notice = Some((message, false));
        }
        actions.favorites.status(ui);
        if refresh {
            self.reset_tracks();
        }
        refresh
    }
}
