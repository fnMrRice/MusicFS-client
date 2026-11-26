use super::super::actions::Actions;
use super::*;
use crate::icons;
use musicfs_server_adapter::Playlist;

const SORT_WIDTH: f32 = 108.;

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
        let width = ui.available_width();
        let mut selected = None;
        let row = egui::ScrollArea::horizontal()
            .id_salt("catalog-tabs")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = if width < 460.0 { 14. } else { 28. };
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
                                .color(ui.visuals().strong_text_color())
                        } else {
                            text.color(style::muted(ui.visuals().dark_mode))
                        };
                        ui.spacing_mut().button_padding = egui::vec2(4., 10.);
                        let r = ui.add(
                            egui::Button::new(text)
                                .frame(false)
                                .min_size(egui::vec2(0., 46.)),
                        );
                        r.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                active,
                                tab.label(),
                            )
                        });
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

    pub(super) fn list_search(&mut self, ui: &mut egui::Ui) {
        let clear_width = self.clear_search_width(ui);
        let after = if ui.layout().main_dir().is_horizontal() {
            36. + ui.spacing().item_spacing.x + clear_width
        } else {
            0.
        };
        self.search_field(ui, (available_width(ui) - after).clamp(100., 190.));
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

    fn filters(ui: &mut egui::Ui) {
        ui.scope(|ui| {
            style::checkbox_style(ui);
            ui.spacing_mut().item_spacing.x = 5.;
            ui.spacing_mut().interact_size.x = 16.;
            ui.add_enabled(false, egui::Checkbox::new(&mut false, tr("仅可播放")))
                .on_disabled_hover_text(tr("等待服务端提供全库可播放过滤"));
        });
        ui.add_enabled(false, egui::Button::new(tr("更多筛选")))
            .on_disabled_hover_text(tr("等待服务端提供组合筛选"));
    }

    fn all_track_controls(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        actions: &mut Actions,
    ) -> bool {
        let before = self.list_query.clone();
        if self.matching.selected_tracks().is_empty() {
            Self::filters(ui);
            if ui.button(tr("新建曲目")).clicked() {
                self.creation = Some(Default::default());
            }
            self.matching.buttons(ui, svc);
        }
        let trailing =
            36. + SORT_WIDTH + 2. * ui.spacing().item_spacing.x + self.clear_search_width(ui);
        let available = available_width(ui) - trailing;
        let field_width = available.clamp(90., 190.);
        ui.add_space((available - field_width).max(0.));
        self.search_field(ui, field_width);
        let refresh = icons::button(ui, "refresh", tr("刷新列表"), 36., 18.).clicked();
        if refresh {
            self.providers.refresh();
            self.artwork = Default::default();
            actions.favorites.refresh();
        }
        refresh || self.list_query != before
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
        let matching = all || matches!(query, TrackQuery::Category { .. } | TrackQuery::Library(_));
        let mut refresh = false;
        if !all && matching {
            ui.horizontal_wrapped(|ui| self.matching.buttons(ui, svc));
        }
        self.poll_queue(svc, actions);
        let album = match query {
            TrackQuery::Category {
                tab: CategoryTab::Album,
                id,
                ..
            } => Some(id),
            _ => None,
        };
        let width = available_width(ui);
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
            } else {
                ui.strong(tr("已选择 {0} 项").replace("{0}", &checked.len().to_string()));
                if ui
                    .add_enabled(
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
                if ui
                    .add_enabled(
                        !actions.busy() && checked.iter().all(|t| !t.unresolved),
                        egui::Button::new(tr("添加到歌单")),
                    )
                    .clicked()
                {
                    actions.open_many(checked, lists);
                }
                if ui
                    .add_enabled(
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
                    ui.menu_button(tr("曲目关联"), |ui| {
                        self.matching.selection_buttons(ui, true)
                    });
                } else if matching {
                    self.matching.selection_buttons(ui, album.is_none());
                }
                if ui.button(tr("清空选择")).clicked() {
                    self.matching.clear_selection();
                }
            }
            if !all && icons::button(ui, "refresh", tr("刷新列表"), 32., 17.).clicked() {
                self.artwork = Default::default();
                actions.favorites.refresh();
                refresh = true;
            }
            if self.tracks.busy() || self.complete.is_some() {
                ui.spinner();
            }
            if all {
                refresh |= self.all_track_controls(ui, svc, actions);
            } else {
                let remain = available_width(ui);
                if width >= 520. && remain > 116. {
                    ui.add_space(remain - 116.);
                }
            }
            providers::disabled_select(
                ui,
                "track-sort",
                tr("默认顺序"),
                SORT_WIDTH,
                16.,
                tr("等待服务端提供完整查询排序能力"),
            );
        };
        if all {
            ui.horizontal(row);
            refresh |= self.creation_status(ui, svc);
        } else {
            ui.add_space(17.);
            ui.horizontal_wrapped(row);
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
