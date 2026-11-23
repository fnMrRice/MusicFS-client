mod browser;
#[cfg(test)]
#[path = "../../../../../tests/native/settings_sources.rs"]
mod tests;
use super::{
    api::{get, path_id, text, Load},
    scans, Services,
};
use crate::i18n::tr;
use eframe::egui;
use musicfs_server_adapter::ServerSession;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
struct Options {
    recursive: bool,
    max_depth: u8,
    import_lyrics: bool,
    import_artwork: bool,
    exclude_patterns: Vec<String>,
    skip_hidden: bool,
    respect_no_media: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            recursive: true,
            max_depth: 0,
            import_lyrics: true,
            import_artwork: false,
            exclude_patterns: Vec::new(),
            skip_hidden: true,
            respect_no_media: true,
        }
    }
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Root {
    pub id: String,
    pub name: String,
    import_defaults: Option<Options>,
}
#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ImportSettings {
    import_defaults: Options,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Config {
    pub revision: String,
    pub watch: Option<super::watch::Policy>,
    pub roots: Vec<Root>,
    #[serde(default)]
    root_import_defaults_supported: bool,
    #[serde(default)]
    import_options: Option<ImportSettings>,
}
pub(super) enum Action {
    Add,
    Edit(String),
    Remove(String),
}

pub(super) struct Editor {
    config: Config,
    action: Action,
    name: String,
    options: Options,
    browser: browser::Browser,
    selected: Vec<(String, String, String)>,
    save: Load<Outcome>,
    key: Option<String>,
    sent: Option<Value>,
}
pub(super) struct Outcome {
    pub scan: Option<Result<String, String>>,
}
pub(super) enum Event {
    Close,
    Saved(Outcome),
    Reload,
}
impl Editor {
    pub fn new(config: Config, action: Action) -> Self {
        let root = match &action {
            Action::Edit(id) | Action::Remove(id) => config.roots.iter().find(|r| &r.id == id),
            _ => None,
        };
        let name = root.map(|r| r.name.clone()).unwrap_or_default();
        let options = root
            .and_then(|r| r.import_defaults.clone())
            .or_else(|| {
                config
                    .import_options
                    .as_ref()
                    .map(|s| s.import_defaults.clone())
            })
            .unwrap_or_default();
        Self {
            config,
            action,
            name,
            options,
            browser: Default::default(),
            selected: vec![],
            save: Default::default(),
            key: None,
            sent: None,
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, svc: Services<'_>, instance: &str) -> Option<Event> {
        if self.save.poll() {
            return self.save.value.take().map(Event::Saved);
        }
        let mut event = None;
        let title = match self.action {
            Action::Add => "添加本地目录",
            Action::Edit(_) => "编辑目录配置",
            Action::Remove(_) => "删除本地目录",
        };
        egui::Window::new(tr(title)).id(egui::Id::new(("edit-music-directory",instance))).collapsible(false)
        .default_width(495.).max_height((ui.ctx().screen_rect().height()-90.).max(180.)).vscroll(true).show(ui.ctx(),|ui|{
            let uncertain=self.sent.is_some();
            let mut submit=false;
            ui.add_enabled_ui(!self.save.busy()&&!uncertain,|ui|{
                match self.action {
                    Action::Add=>{
                        ui.weak(tr("只能选择服务端允许访问的目录或其子目录。这里显示的是服务端的文件夹。"));
                        self.browser.show(ui,svc,instance,&self.config.roots,&mut self.selected);
                    },
                    Action::Edit(_)=>{ui.label(tr("目录名称")); ui.text_edit_singleline(&mut self.name);},
                    Action::Remove(_)=>{ui.label(format!("{} · {}",tr("删除本地目录"),self.name));ui.weak(tr("仅移除此音乐导入目录，磁盘文件和已有曲库记录会保留，允许访问的目录不受影响。"));}
                }
                if !matches!(self.action,Action::Remove(_)) {
                    if self.config.root_import_defaults_supported {options_ui(ui,&mut self.options);}
                    else {ui.weak(tr("此服务端暂不支持单独配置目录查找设置。"));}
                }
                let can=match &self.action {
                    Action::Add=>!self.selected.is_empty(),
                    Action::Remove(id)=>self.config.roots.iter().any(|r|&r.id==id),
                    Action::Edit(id)=>self.config.roots.iter().find(|r|&r.id==id).is_some_and(|r|
                        !self.name.trim().is_empty() && (self.name.trim()!=r.name || self.options!=self.original_options(r)))
                };
                if ui.add_enabled(can,egui::Button::new(tr(match self.action {Action::Add=>"添加所选目录",Action::Remove(_)=>"删除",_=>"保存"}))).clicked(){submit=true;}
            });
            if self.save.busy(){ ui.spinner();ui.weak(tr("正在应用目录设置…")); }
            if let Some(e)=&self.save.error {
                ui.colored_label(ui.visuals().error_fg_color,e);
                let conflict=e.contains("HTTP 412")||e.contains("HTTP 409")||e.contains("superseded")||e.contains("failed");
                if conflict {
                    if ui.button(tr("重新读取配置并放弃未保存选择")).clicked(){event=Some(Event::Reload);}
                }else if ui.button(tr("继续确认保存结果")).clicked(){submit=true;}
            }
            if !self.save.busy() && ui.button(tr("取消")).clicked(){event=Some(Event::Close);}
            if submit {self.submit(ui,svc,instance);}
        });
        event
    }
    fn original_options(&self, r: &Root) -> Options {
        r.import_defaults
            .clone()
            .or_else(|| {
                self.config
                    .import_options
                    .as_ref()
                    .map(|s| s.import_defaults.clone())
            })
            .unwrap_or_default()
    }
    fn body(&self) -> Value {
        let mut roots = Vec::new();
        for r in &self.config.roots {
            if matches!(&self.action,Action::Remove(id) if id==&r.id) {
                continue;
            }
            let edit = matches!(&self.action,Action::Edit(id) if id==&r.id);
            let mut v = json!({"id":r.id,"name":if edit{self.name.trim()}else{&r.name}});
            // 省略已有目录未修改的选项，由服务端保留继承或显式覆盖关系。
            if edit
                && self.config.root_import_defaults_supported
                && self.options != self.original_options(r)
            {
                v["importDefaults"] = json!(self.options);
            }
            roots.push(v);
        }
        if matches!(self.action, Action::Add) {
            for (id, name, path) in &self.selected {
                let mut v = json!({"id":id,"name":name,"path":path});
                if self.config.root_import_defaults_supported {
                    v["importDefaults"] = json!(self.options);
                }
                roots.push(v);
            }
        }
        json!({"musicRoots":roots,"importDefaults":self.config.import_options.clone().unwrap_or_default().import_defaults})
    }
    fn submit(&mut self, ui: &egui::Ui, svc: Services<'_>, id: &str) {
        let body = self.sent.clone().unwrap_or_else(|| self.body());
        self.sent = Some(body.clone());
        let key = self
            .key
            .get_or_insert_with(|| Uuid::new_v4().to_string())
            .clone();
        let (s, id, rev, scan) = (
            svc.server.clone(),
            id.to_owned(),
            self.config.revision.clone(),
            !matches!(self.action, Action::Remove(_)),
        );
        self.save.start(ui, svc, async move {
            let op = apply(&s, &id, &rev, &key, body).await?;
            Ok(Outcome {
                scan: if scan {
                    Some(scans::auto_scan(&s, &id, &op).await)
                } else {
                    None
                },
            })
        });
    }
}
pub(super) async fn apply(
    s: &ServerSession,
    id: &str,
    revision: &str,
    key: &str,
    body: Value,
) -> Result<String, String> {
    let url = format!("/admin/plugin-instances/{}/configuration", path_id(id));
    let mut op = s
        .settings_json(
            "PUT",
            &url,
            Some(&body),
            &[("If-Match", revision), ("Idempotency-Key", key)],
        )
        .await?;
    let op_id = op["operationId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Missing configuration operation.")?
        .to_owned();
    loop {
        if text(&op, "operationId") != op_id {
            return Err("Unexpected directory operation.".into());
        }
        match text(&op, "status") {
            "succeeded" => return Ok(op_id),
            "applying" | "draining" => {}
            "failed" | "superseded" => {
                return Err(format!(
                    "Directory operation {}: {}",
                    text(&op, "status"),
                    text(&op, "errorCode")
                ))
            }
            _ => return Err("Invalid directory operation status.".into()),
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        op = get(s, &format!("/admin/plugin-operations/{}", path_id(&op_id))).await?;
    }
}
fn options_ui(ui: &mut egui::Ui, o: &mut Options) {
    ui.separator();
    ui.strong(tr("查找设置"));
    if ui
        .checkbox(&mut o.recursive, tr("递归查找子目录"))
        .changed()
        && !o.recursive
    {
        o.max_depth = 0;
    }
    ui.checkbox(&mut o.import_lyrics, tr("导入歌词"));
    ui.collapsing(tr("高级选项"), |ui| {
        ui.add_enabled_ui(o.recursive, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("递归深度"));
                ui.add(egui::DragValue::new(&mut o.max_depth).range(0..=64));
                ui.weak(tr("0 表示全部递归"));
            })
        });
        let mut hidden = !o.skip_hidden;
        if ui.checkbox(&mut hidden, tr("包含隐藏文件和目录")).changed() {
            o.skip_hidden = !hidden;
        }
        ui.checkbox(&mut o.respect_no_media, tr("遵循 .nomedia 标记"));
        ui.checkbox(&mut o.import_artwork, tr("导入曲目封面"));
    });
}
