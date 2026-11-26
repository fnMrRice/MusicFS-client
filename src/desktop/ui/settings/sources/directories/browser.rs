use super::{get, path_id, Load, Root, Services};
use crate::i18n::tr;
use eframe::egui;
use serde::Deserialize;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    name: String,
    path: String,
    available: bool,
    configured_root_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    revision: String,
    items: Vec<Item>,
    next_cursor: Option<String>,
}
#[derive(Default)]
pub(super) struct Browser {
    path: Vec<String>,
    entries: Vec<Item>,
    page: Load<Page>,
    next: Option<String>,
    revision: Option<String>,
    seen: HashSet<String>,
}
impl Browser {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        instance: &str,
        roots: &[Root],
        selected: &mut Vec<(String, String, String)>,
    ) {
        if self.page.poll() {
            let page = self.page.value.take().unwrap();
            if self.revision.as_ref().is_some_and(|r| *r != page.revision)
                || page
                    .next_cursor
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || !self.seen.insert(s.clone()))
            {
                self.page.error = Some(tr("目录列表已变化，请重新读取当前目录。").into());
                self.next = None;
            } else {
                self.revision = Some(page.revision);
                self.next = page.next_cursor;
                self.entries.extend(page.items);
            }
        }
        let mut load = self.revision.is_none() && self.page.initial();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.path.is_empty(), egui::Button::new(tr("返回上一级")))
                .clicked()
            {
                self.path.pop();
                self.reset();
                load = true;
            }
            ui.label(
                self.path
                    .last()
                    .map(String::as_str)
                    .unwrap_or_else(|| tr("允许访问的目录")),
            );
        });
        if self.page.status(ui) {
            self.reset();
            load = true;
        }
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt("source-directories")
            .max_height(220.)
            .show(ui, |ui| {
                for item in &self.entries {
                    ui.push_id(&item.path, |ui| {
                        ui.horizontal(|ui| {
                            let saved = item
                                .configured_root_id
                                .as_ref()
                                .is_some_and(|id| roots.iter().any(|r| &r.id == id));
                            let mut on = saved || selected.iter().any(|(_, _, p)| p == &item.path);
                            if ui
                                .add_enabled(
                                    !saved && item.available,
                                    egui::Checkbox::new(&mut on, &item.name),
                                )
                                .changed()
                            {
                                if on {
                                    selected.push((
                                        format!("root-{}", Uuid::new_v4()),
                                        item.name.clone(),
                                        item.path.clone(),
                                    ));
                                } else {
                                    selected.retain(|(_, _, p)| p != &item.path);
                                }
                            }
                            if ui
                                .add_enabled(item.available, egui::Button::new(tr("打开")))
                                .clicked()
                            {
                                open = Some(item.path.clone());
                            }
                        })
                    });
                }
                if self.entries.is_empty()
                    && self.next.is_none()
                    && !self.page.busy()
                    && self.page.error.is_none()
                    && self.revision.is_some()
                {
                    ui.weak(tr("此目录没有可浏览的子目录。"));
                }
                if self.next.is_some()
                    && !self.page.busy()
                    && self.page.error.is_none()
                    && crate::ui::page_end_visible(ui)
                {
                    load = true;
                }
            });
        if let Some(path) = open {
            self.path.push(path);
            self.reset();
            load = true;
        }
        if load {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            if let Some(p) = self.path.last() {
                query.append_pair("path", p);
            }
            if let Some(c) = &self.next {
                query.append_pair("cursor", c);
            }
            let path = format!(
                "/admin/plugin-instances/{}/directories?{}",
                path_id(instance),
                query.finish()
            );
            let server = svc.server.clone();
            self.page
                .start(ui, svc, async move { get(&server, &path).await });
        }
        ui.label(tr("待添加的目录"));
        let mut remove = None;
        for (i, (_, name, path)) in selected.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(name).on_hover_text(path);
                if ui.small_button("−").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            selected.remove(i);
        }
    }
    fn reset(&mut self) {
        self.page = Load::default();
        self.entries.clear();
        self.next = None;
        self.revision = None;
        self.seen.clear();
    }
}
