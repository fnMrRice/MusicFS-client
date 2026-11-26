use super::entries::Entries;
use crate::{
    i18n::tr,
    icons,
    ui::{
        data::{Pages, Services},
        style,
    },
};
use eframe::egui::{self, RichText};
use musicfs_server_adapter::{Page, Playlist};
use serde_json::{json, Value};

type Choices = Pages<(String, String)>;

#[derive(Default)]
pub(super) struct Rules {
    personal: Choices,
    remote: Choices,
    entries: Option<(usize, Entries)>,
    picker: bool,
}

enum Change {
    Move(usize, usize),
    Remove(usize),
    Inspect(usize),
}

impl Rules {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        lists: &Pages<Playlist>,
        own: Option<&str>,
        rules: &mut Vec<Value>,
    ) -> bool {
        let mut changed = false;
        let mut change = None;
        if self.picker || rules.iter().any(|r| r["targetId"].is_string()) {
            load_choices(ui, svc, &mut self.personal, false);
            load_choices(ui, svc, &mut self.remote, true);
        }
        if rules.is_empty() {
            ui.weak(tr("可加入其他歌单，也可以稍后添加曲目。"));
        }
        let count = rules.len();
        for (index, rule) in rules.iter_mut().enumerate() {
            let kind = rule["kind"].as_str().unwrap_or("");
            let id = rule["targetId"].as_str().unwrap_or("");
            let entries = rule["entries"].as_array().map_or(0, Vec::len);
            let local = kind == "local";
            let remote = kind == "remote";
            let title = if local {
                tr("已选曲目 · {0} 首").replace("{0}", &entries.to_string())
            } else if kind == "playlist" {
                self.personal
                    .items
                    .iter()
                    .find(|(pid, _)| pid == id)
                    .map(|(_, name)| name.as_str())
                    .or_else(|| {
                        lists
                            .items
                            .iter()
                            .find(|p| p.id == id)
                            .map(|p| p.name.as_str())
                    })
                    .unwrap_or(id)
                    .to_owned()
            } else {
                self.remote
                    .items
                    .iter()
                    .find(|(pid, _)| pid == id)
                    .map_or(id, |(_, title)| title)
                    .to_owned()
            };
            ui.push_id(index, |ui| {
                let dark = ui.visuals().dark_mode;
                egui::Frame::new()
                    .fill(style::background(dark))
                    .corner_radius(7)
                    .inner_margin(7)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let name_width = (ui.available_width() - 140.).max(40.);
                            ui.allocate_ui_with_layout(
                                egui::vec2(name_width, 28.),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    if local && entries > 0 {
                                        let open = self
                                            .entries
                                            .as_ref()
                                            .is_some_and(|(row, _)| *row == index);
                                        let arrow = if open { "chevron-up" } else { "chevron" };
                                        if icons::button(ui, arrow, tr("编辑曲目来源"), 24., 14.)
                                            .clicked()
                                        {
                                            change = Some(Change::Inspect(index));
                                        }
                                    } else {
                                        let (icon, _) = ui.allocate_exact_size(
                                            egui::Vec2::splat(24.),
                                            egui::Sense::hover(),
                                        );
                                        icons::paint(
                                            ui,
                                            if remote { "server" } else { "queue" },
                                            icon.shrink(2.),
                                            style::muted(dark),
                                        );
                                    }
                                    ui.add(egui::Label::new(&title).truncate())
                                        .on_hover_text(&title);
                                },
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button(tr("移除")).clicked() {
                                        change = Some(Change::Remove(index));
                                    }
                                    ui.add_enabled_ui(index + 1 < count, |ui| {
                                        if icons::button(ui, "chevron", tr("下移规则"), 24., 14.)
                                            .clicked()
                                        {
                                            change = Some(Change::Move(index, index + 1));
                                        }
                                    });
                                    ui.add_enabled_ui(index > 0, |ui| {
                                        if icons::button(ui, "chevron-up", tr("上移规则"), 24., 14.)
                                            .clicked()
                                        {
                                            change = Some(Change::Move(index, index - 1));
                                        }
                                    });
                                },
                            );
                        });
                        if let Some((row, page)) = &mut self.entries {
                            if *row == index {
                                if let Some(entries) = rule["entries"].as_array_mut() {
                                    changed |= page.show(ui, svc, entries);
                                }
                            }
                        }
                    });
            });
        }
        match change {
            Some(Change::Move(from, to)) => {
                rules.swap(from, to);
                self.entries = None;
                changed = true;
            }
            Some(Change::Remove(at)) => {
                rules.remove(at);
                self.entries = None;
                changed = true;
            }
            Some(Change::Inspect(at)) => {
                if self.entries.as_ref().is_some_and(|(row, _)| *row == at) {
                    self.entries = None;
                } else {
                    self.entries = Some((at, Entries::default()));
                }
            }
            None => {}
        }
        if ui.button(tr("添加歌单来源")).clicked() {
            self.picker = !self.picker;
        }
        if self.picker {
            let mut picked = None;
            ui.horizontal_wrapped(|ui| {
                picked = choice_menu(ui, tr("个人歌单"), "playlist", &mut self.personal, own);
                if let Some(value) =
                    choice_menu(ui, tr("已导入的在线歌单"), "remote", &mut self.remote, None)
                {
                    picked = Some(value);
                }
            });
            if let Some(rule) = picked {
                rules.push(rule);
                self.picker = false;
                changed = true;
            }
        }
        changed
    }
}

fn choice_menu(
    ui: &mut egui::Ui,
    label: &str,
    kind: &str,
    choices: &mut Choices,
    exclude: Option<&str>,
) -> Option<Value> {
    let mut result = None;
    egui::ComboBox::from_id_salt(("playlist-target", kind))
        .width(195.)
        .selected_text(label)
        .show_ui(ui, |ui| {
            let mut empty = true;
            for (id, name) in &choices.items {
                if Some(id.as_str()) == exclude {
                    continue;
                }
                empty = false;
                if ui
                    .add_enabled(true, egui::Button::new(name).frame(false).truncate())
                    .on_hover_text(name)
                    .clicked()
                {
                    result = Some(json!({"kind":kind, "targetId":id, "entries":[]}));
                    ui.close_menu();
                }
            }
            if let Some(error) = &choices.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
                if ui.button(tr("重试")).clicked() {
                    *choices = Choices::default();
                }
            } else if choices.busy() || !choices.started {
                ui.spinner();
            } else if empty {
                ui.label(RichText::new(tr("没有可选歌单")).weak());
            }
        });
    result
}

fn load_choices(ui: &egui::Ui, svc: Services<'_>, choices: &mut Choices, remote: bool) {
    choices.poll();
    if choices.busy() || choices.error.is_some() || (choices.started && choices.next.is_none()) {
        return;
    }
    let server = svc.server.clone();
    let cursor = choices.next.clone();
    choices.load(ui.ctx(), svc.runtime, async move {
        if remote {
            let page = server.imported_playlists(cursor.as_deref()).await?;
            Ok(Page {
                next_cursor: page.next_cursor,
                items: page
                    .items
                    .into_iter()
                    .map(|p| {
                        let name = if p.title.is_empty() {
                            p.external_id
                        } else {
                            p.title
                        };
                        (p.playlist_id, name)
                    })
                    .collect(),
            })
        } else {
            let page = server.playlists(cursor.as_deref()).await?;
            Ok(Page {
                next_cursor: page.next_cursor,
                items: page.items.into_iter().map(|p| (p.id, p.name)).collect(),
            })
        }
    });
}
