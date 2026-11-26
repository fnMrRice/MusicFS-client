use crate::ui::style;
use crate::ui::style::ComboBoxMotion;
mod metadata;
#[cfg(test)]
#[path = "../../../tests/native/album_merge.rs"]
mod tests;
use super::data::Services;
use crate::{i18n::tr, task::Task};
use eframe::egui;
use metadata::{Draft, Field, Item, FIELDS};
use musicfs_server_adapter::{ServerSession, Track};
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Default)]
pub(super) struct AlbumMerge {
    dialog: Option<Dialog>,
}
struct Dialog {
    album: String,
    ids: Vec<String>,
    read: Option<Task<Selection>>,
    data: Option<Selection>,
    target: usize,
    // 每个字段分别保留自动值、指定曲目的值或用户输入。
    choices: [Choice; 7],
    custom: Draft,
    save: Option<Task<()>>,
    retry: Option<(Value, String)>,
    conflict: bool,
    error: Option<String>,
}
#[derive(Default, Clone, Copy, PartialEq)]
enum Choice {
    #[default]
    Auto,
    Track(usize),
    Custom,
}
struct Selection {
    items: Vec<Item>,
    album_revision: u64,
    matching_revision: u64,
}
impl AlbumMerge {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        album_id: &str,
        checked: &[Track],
    ) -> bool {
        let count = checked.len();
        if style::add_button(
            ui,
            (2..=64).contains(&count) && !album_id.is_empty() && self.dialog.is_none(),
            egui::Button::new(tr("合并歌曲")),
        )
        .clicked()
        {
            let ids = checked.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
            let mut dialog = Dialog {
                album: album_id.into(),
                ids,
                read: None,
                data: None,
                target: 0,
                choices: [Choice::Auto; 7],
                custom: Default::default(),
                save: None,
                retry: None,
                conflict: false,
                error: None,
            };
            dialog.read(ui, svc);
            self.dialog = Some(dialog);
        }
        let Some(dialog) = &mut self.dialog else {
            return false;
        };
        if dialog.album != album_id {
            self.dialog = None;
            return false;
        }
        let result = dialog.show(ui, svc);
        if let Some(refresh) = result {
            self.dialog = None;
            return refresh;
        }
        false
    }
}
impl Dialog {
    fn read(&mut self, ui: &egui::Ui, svc: Services<'_>) {
        let (s, album, ids) = (svc.server.clone(), self.album.clone(), self.ids.clone());
        self.error = None;
        self.read = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
            load_selection(&s, &album, &ids).await
        }));
    }
    fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>) -> Option<bool> {
        if let Some(result) = self.read.as_mut().and_then(Task::poll) {
            self.read = None;
            match result {
                Ok(data) => {
                    self.target = data
                        .items
                        .iter()
                        .position(|i| i.versions() > 0)
                        .unwrap_or(0);
                    self.data = Some(data);
                }
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(result) = self.save.as_mut().and_then(Task::poll) {
            self.save = None;
            match result {
                Ok(()) => return Some(true),
                Err(e) => {
                    self.conflict = e.contains("HTTP 409") || e.contains("HTTP 412");
                    if e.contains("HTTP 4") {
                        self.retry = None;
                    }
                    self.error = Some(if e.contains("ALBUM_TRACK_SLOT_CONFLICT") {
                        tr("所选歌曲在另一张专辑中占用不同曲序，请先核对那张专辑。").into()
                    } else {
                        e
                    });
                }
            }
        }
        let mut response = None;
        egui::Window::new(tr("合并歌曲"))
            .id(egui::Id::new("album-merge-selection"))
            .default_width(600.)
            .max_height((ui.ctx().screen_rect().height() - 86.).max(200.))
            .vscroll(true)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                if self.read.is_some() || self.save.is_some() {
                    ui.spinner();
                }
                if let Some(err) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, err);
                }
                if self.data.is_none()
                    && self.read.is_none()
                    && style::button(ui, tr("重试")).clicked()
                {
                    self.read(ui, svc);
                }
                let locked = self.conflict || self.save.is_some() || self.retry.is_some();
                if let Some(data) = &self.data {
                    let mut wire = None;
                    ui.add_enabled_ui(!locked, |ui| {
                        ui.strong(tr("保留的曲目"));
                        for (i, item) in data.items.iter().enumerate() {
                            let count = item.versions();
                            let versions = if count == 0 {
                                tr("暂无来源").into()
                            } else {
                                tr("{0} 个来源版本").replace("{0}", &count.to_string())
                            };
                            style::radio_value(
                                ui,
                                &mut self.target,
                                i,
                                format!("{} · {}", item.title, versions),
                            );
                        }
                        ui.separator();
                        ui.strong(tr("合并后的元数据"));
                        ui.weak(tr("每项可选用任一曲目的值，也可以直接编辑。"));
                        let result = edit_fields(
                            ui,
                            &data.items,
                            self.target,
                            &mut self.choices,
                            &mut self.custom,
                        );
                        match result.wire() {
                            Ok(v) => wire = Some(v),
                            Err(e) => {
                                ui.colored_label(ui.visuals().error_fg_color, tr(e));
                            }
                        }
                    });
                    if self.conflict {
                        if style::button(ui, tr("刷新列表并重新选择")).clicked() {
                            response = Some(true);
                        }
                    } else if style::add_button(
                        ui,
                        self.save.is_none() && (self.retry.is_some() || wire.is_some()),
                        egui::Button::new(tr(if self.retry.is_some() {
                            "重试确认保存"
                        } else {
                            "确认合并"
                        })),
                    )
                    .clicked()
                    {
                        let (body, key) = self.retry.clone().unwrap_or_else(|| {
                            (
                                payload(data, self.target, wire.unwrap()),
                                Uuid::new_v4().to_string(),
                            )
                        });
                        let (s, album) = (svc.server.clone(), self.album.clone());
                        self.retry = Some((body.clone(), key.clone()));
                        self.error = None;
                        self.save = Some(Task::spawn(svc.runtime, ui.ctx(), async move {
                            save(&s, &album, &body, &key).await
                        }));
                    }
                }
                if style::add_button(ui, self.save.is_none(), egui::Button::new(tr("取消")))
                    .clicked()
                {
                    response = Some(false);
                }
            });
        response
    }
}

fn edit_fields(
    ui: &mut egui::Ui,
    items: &[Item],
    target: usize,
    choices: &mut [Choice; 7],
    custom: &mut Draft,
) -> Draft {
    let drafts: Vec<_> = items.iter().map(Item::draft).collect();
    let mut automatic = Draft::auto(items, target, None);
    let mut result = automatic.clone();
    for (i, field) in FIELDS.into_iter().enumerate() {
        ui.push_id(i, |ui| {
            let choice = &mut choices[i];
            ui.horizontal_wrapped(|ui| {
                ui.label(field.label());
                egui::ComboBox::from_id_salt("metadata-value-source")
                    .selected_text(match *choice {
                        Choice::Auto => tr("自动合并"),
                        Choice::Custom => tr("自定义"),
                        Choice::Track(n) => &items[n].title,
                    })
                    .show_animated(ui, |ui| {
                        style::selectable_value(ui, choice, Choice::Auto, tr("自动合并"));
                        for (n, original) in drafts.iter().enumerate() {
                            let v = original.preview(field);
                            let display = if v.is_empty() { tr("未填写") } else { &v };
                            style::selectable_value(
                                ui,
                                choice,
                                Choice::Track(n),
                                format!("{}. {} · {}", n + 1, items[n].title, display),
                            );
                        }
                    });
            });
            result.assign(
                field,
                match *choice {
                    Choice::Auto => &automatic,
                    Choice::Track(n) => &drafts[n],
                    Choice::Custom => &*custom,
                },
            );
            if result.edit(ui, field) {
                custom.assign(field, &result);
                *choice = Choice::Custom;
            }
            if field == Field::Title && result.title != automatic.title {
                automatic = Draft::auto(items, target, Some(&result.title));
            }
        });
    }
    result
}
fn payload(selection: &Selection, target: usize, meta: Value) -> Value {
    let members: Vec<_> = selection
        .items
        .iter()
        .map(|t| json!({"trackId":t.track_id,"expectedRevision":t.revision}))
        .collect();
    json!({"expectedAlbumRevision":selection.album_revision,"expectedMatchingRevision":selection.matching_revision,
        "targetTrackId":selection.items[target].track_id,"tracks":members,"metadata":meta})
}
fn path(id: &str) -> String {
    url::form_urlencoded::byte_serialize(id.as_bytes()).collect()
}
async fn load_selection(
    s: &ServerSession,
    album: &str,
    ids: &[String],
) -> Result<Selection, String> {
    if !(2..=64).contains(&ids.len()) || ids.iter().collect::<HashSet<_>>().len() != ids.len() {
        return Err(tr("请刷新专辑后重新选择，单次最多合并 64 首。").into());
    }
    let album_info = s
        .settings_json(
            "GET",
            &format!("/library/albums/{}", path(album)),
            None,
            &[],
        )
        .await?;
    if album_info["albumId"] != album {
        return Err("Unexpected album response.".into());
    }
    let album_revision = album_info["revision"]
        .as_u64()
        .filter(|r| *r > 0)
        .ok_or("Invalid album revision.")?;
    let matching_revision = s.matching_revision().await?;
    let mut items = Vec::new();
    // 同时读取至多八项，保留表格选择的顺序。
    for group in ids.chunks(8) {
        let mut requests = tokio::task::JoinSet::new();
        for (index, id) in group.iter().enumerate() {
            let (s, id) = (s.clone(), id.clone());
            requests.spawn(async move {
                let v = s
                    .settings_json("GET", &format!("/library/tracks/{}", path(&id)), None, &[])
                    .await?;
                let t: Item =
                    serde_json::from_value(v).map_err(|_| "Invalid MusicFS track metadata.")?;
                if t.track_id != id || t.revision == 0 {
                    return Err("Unexpected track response.".to_owned());
                }
                Ok((index, t))
            });
        }
        let mut part = Vec::new();
        while let Some(res) = requests.join_next().await {
            let (index, t) = res.map_err(|_| "Metadata request interrupted.")??;
            if !t.in_album(album) {
                return Err(tr("所选曲目已不在此专辑中，请刷新列表并重新选择。").into());
            }
            part.push((index, t));
        }
        part.sort_by_key(|(index, _)| *index);
        items.extend(part.into_iter().map(|(_, t)| t));
    }
    Ok(Selection {
        album_revision,
        matching_revision,
        items,
    })
}
async fn save(s: &ServerSession, album: &str, body: &Value, key: &str) -> Result<(), String> {
    let result = s
        .settings_json(
            "POST",
            &format!("/library/albums/{}/merge-tracks", path(album)),
            Some(body),
            &[("Idempotency-Key", key)],
        )
        .await?;
    let track: Item =
        serde_json::from_value(result).map_err(|_| "Invalid merged track response.")?;
    if track.track_id.is_empty() || track.revision == 0 || !track.in_album(album) {
        return Err("Unexpected merged track response.".into());
    }
    Ok(())
}
