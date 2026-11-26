use super::{
    api::{get, path_id, text, Load},
    directories::{Action, Config, Editor, Event},
    scans::Scans,
    Services,
};
use crate::{i18n::tr, ui::style};
use eframe::egui;
use serde_json::Value;

#[derive(Default)]
pub(super) struct Local {
    config: Load<Config>,
    stats: Load<Value>,
    scans: Scans,
    watch: super::watch::Watch,
    editor: Option<Editor>,
    queued: Option<Action>,
    error: Option<String>,
}
impl Local {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        svc: Services<'_>,
        instance: &Value,
        provider: Option<&Value>,
        open: bool,
        refresh: bool,
    ) {
        let id = text(instance, "instanceId");
        if open {
            self.queued = Some(Action::Add);
        }
        self.config.poll();
        self.stats.poll();
        let ready = text(instance, "runtimeState") == "ready"
            && provider.is_some_and(|p| {
                matches!(text(p, "availability"), "available" | "online")
                    && p["capabilities"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|c| c == "library.scan"))
            });
        let finished = self.scans.show(ui, svc, id, refresh);
        let reload = self.config.status(ui) || self.config.initial() || refresh;
        if reload {
            let (s, id) = (svc.server.clone(), path_id(id));
            self.config.start(ui, svc, async move {
                get(&s, &format!("/admin/plugin-instances/{id}/configuration")).await
            });
        }
        let retry_stats = ui
            .push_id("root-statistics", |ui| self.stats.status(ui))
            .inner;
        if self.stats.initial() || retry_stats || finished || refresh {
            let (s, id) = (svc.server.clone(), path_id(id));
            self.stats.start(ui, svc, async move {
                get(&s, &format!("/provider-instances/{id}/root-statistics")).await
            });
        }
        let Some(config) = &self.config.value else {
            return;
        };
        let reload_watch = self.watch.show(ui, svc, id, config);
        let mut action = self.queued.take();
        ui.horizontal_wrapped(|ui| {
            if style::button(ui, tr("添加本地目录")).clicked() {
                action = Some(Action::Add);
            }
            if style::add_button(
                ui,
                ready && config.roots.iter().any(|r| !self.scans.active(&r.id)),
                egui::Button::new(tr("刷新全部目录")),
            )
            .clicked()
            {
                for r in &config.roots {
                    self.scans.refresh(ui, svc, id, &r.id, &r.name);
                }
            }
        });
        if !ready {
            ui.weak(tr("本地音乐来源尚未就绪，请先在插件设置中启用并检查状态。"));
        }
        if config.roots.is_empty() {
            ui.weak(tr("暂无已添加目录"));
        }
        for root in &config.roots {
            ui.push_id(&root.id, |ui| {
                super::action_row(
                    ui,
                    |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&root.name).strong()).truncate(),
                        );
                        if let Some(stats) = &self.stats.value {
                            let row = stats["roots"]
                                .as_array()
                                .and_then(|rows| rows.iter().find(|v| v["rootId"] == root.id));
                            let (n, b) = row
                                .map(|v| {
                                    (
                                        v["trackCount"].as_u64().unwrap_or(0),
                                        v["sizeBytes"].as_u64().unwrap_or(0),
                                    )
                                })
                                .unwrap_or((0, 0));
                            ui.weak(format!("{n} {} · {}", tr("首"), bytes(b)));
                        }
                    },
                    |ui| {
                        if style::button(ui, tr("编辑")).clicked() {
                            action = Some(Action::Edit(root.id.clone()));
                        }
                        if style::button(ui, tr("删除")).clicked() {
                            action = Some(Action::Remove(root.id.clone()));
                        }
                        if style::add_button(
                            ui,
                            ready && !self.scans.active(&root.id),
                            egui::Button::new(tr("刷新元数据")),
                        )
                        .clicked()
                        {
                            self.scans.refresh(ui, svc, id, &root.id, &root.name);
                        }
                    },
                );
            });
        }
        if let Some(a) = action {
            if self.editor.is_none() {
                self.editor = Some(Editor::new(config.clone(), a));
            }
        }
        if let Some(event) = self
            .editor
            .as_mut()
            .and_then(|editor| editor.show(ui, svc, id))
        {
            self.editor = None;
            match event {
                Event::Close => {}
                Event::Reload => {
                    self.config = Load::default();
                }
                Event::Saved(outcome) => {
                    self.config = Load::default();
                    self.stats = Load::default();
                    self.error = None;
                    if let Some(scan) = outcome.scan {
                        match scan {
                            Ok(id) => self.scans.monitor(id),
                            Err(e) => self.error = Some(e),
                        }
                    }
                }
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if reload_watch {
            self.config = Load::default();
        }
    }
}
fn bytes(n: u64) -> String {
    let mut size = n as f64;
    let mut unit = 0;
    while size >= 1024. && unit < 4 {
        size /= 1024.;
        unit += 1;
    }
    format!("{size:.1} {}", ["B", "KiB", "MiB", "GiB", "TiB"][unit])
}
