use super::catalog::{localized, purpose, Instance, Online, Plugin};
use crate::i18n::tr;
use eframe::egui;
use std::collections::BTreeSet;

pub(super) enum Command {
    Configure,
    Directory(String),
    Switch {
        instance: Option<String>,
        enabled: bool,
    },
}

pub(super) fn show(
    ui: &mut egui::Ui,
    plugin: &Plugin,
    online: Option<&Online>,
    expanded: &mut bool,
    writable: bool,
) -> Option<Command> {
    let mut result = None;
    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            let arrow = if *expanded { "▾" } else { "▸" };
            if ui
                .selectable_label(*expanded, format!("{arrow}  {}", plugin.name()))
                .clicked()
            {
                *expanded = !*expanded;
            }
            if let Some(count) = plugin
                .display_count
                .as_ref()
                .filter(|c| !c.unit.trim().is_empty())
            {
                ui.weak(format!(
                    "{} {}",
                    count.value,
                    localized(&count.unit, &count.localized_units)
                ));
            }
            if plugin.version.trim().is_empty() {
                ui.weak(tr("未知版本"));
            } else {
                ui.weak(tr("版本：{0}").replace("{0}", &plugin.version));
            }
            if plugin.settings_available && ui.button(tr("配置")).clicked() {
                result = Some(Command::Configure);
            }
        });
        if !*expanded {
            return;
        }
        ui.separator();
        let purposes = plugin.purposes();
        row(
            ui,
            tr("用途"),
            purposes
                .iter()
                .map(|v| tr(purpose(v)))
                .collect::<Vec<_>>()
                .join(" · "),
        );
        row(ui, tr("插件 ID"), &plugin.plugin_id);
        if purposes.contains("scraper") {
            let fields: BTreeSet<_> = plugin
                .builtin
                .iter()
                .flat_map(|v| v.capabilities.iter().flatten())
                .chain(
                    plugin
                        .instances
                        .iter()
                        .filter(|v| v.role == "scraper")
                        .flat_map(|v| v.capabilities.iter().flatten()),
                )
                .map(|v| tr(field_name(v)))
                .collect();
            if !fields.is_empty() {
                row(
                    ui,
                    tr("支持刮削的内容"),
                    fields.into_iter().collect::<Vec<_>>().join(" · "),
                );
            }
        }
        if plugin.required {
            ui.strong(tr("必需"));
        }
        if plugin.builtin.is_none() && plugin.storage.is_none() {
            if let Some(v) = &plugin.execution {
                row(ui, tr("运行方式"), mode(v));
            }
        }
        if infrastructure(ui, plugin) {
            return;
        }
        let switchable = writable && plugin.state_mutable && !plugin.required;
        if switchable && switch(ui, plugin.configured_enabled, tr("插件开关")) {
            result = Some(Command::Switch {
                instance: None,
                enabled: !plugin.configured_enabled,
            });
        }
        ui.strong(tr("插件实例"));
        if plugin.instances.is_empty() {
            ui.weak(tr("暂无插件实例。"));
        }
        for instance in &plugin.instances {
            ui.push_id(&instance.instance_id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(instance_name(plugin, instance, online));
                    ui.label(tr(state(&instance.runtime_state)));
                });
                if let Some(user) = &instance.owner_user_id {
                    ui.weak(tr("所属用户：{0}").replace("{0}", user));
                }
                if instance.reason.as_deref() == Some("RESTART_REQUIRED") {
                    ui.weak(tr("等待重启后停用。"));
                }
                if instance.runtime_state == "pending_configuration" {
                    ui.weak(tr(match plugin.plugin_id.as_str() {
                        "musicfs.plugin.source.localfiles" => "请从允许访问的目录中选择音乐目录。",
                        "musicfs.plugin.decoder.ffmpeg" => {
                            "请在服务端配置可用的 FFmpeg 和 ffprobe 路径。"
                        }
                        _ => "请在服务端完成插件配置。",
                    }));
                }
                if plugin.plugin_id == "musicfs.plugin.source.localfiles"
                    && ui.link(tr("管理音乐目录")).clicked()
                {
                    result = Some(Command::Directory(instance.instance_id.clone()));
                }
                if switchable && switch(ui, instance.configured_enabled, tr("实例开关")) {
                    result = Some(Command::Switch {
                        instance: Some(instance.instance_id.clone()),
                        enabled: !instance.configured_enabled,
                    });
                }
            });
        }
    });
    result
}

fn switch(ui: &mut egui::Ui, configured: bool, label: &str) -> bool {
    let mut value = configured;
    ui.checkbox(&mut value, label).changed()
}

fn instance_name(plugin: &Plugin, instance: &Instance, online: Option<&Online>) -> String {
    let name = plugin.name();
    if plugin.supports_multiple_instances {
        let account = instance
            .account_name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                let item = online?.items.iter().find(|o| {
                    o.instance_id == instance.instance_id
                        && o.account_id.as_ref().is_some_and(|s| !s.is_empty())
                })?;
                item.name.as_deref().map(str::trim).filter(|s| {
                    !s.is_empty()
                        && *s != name
                        && *s != instance.instance_id
                        && !plugin.plugin.as_ref().is_some_and(|p| p.name == *s)
                })
            });
        if let Some(account) = account {
            return format!("{name} · {account}");
        }
    }
    name.to_owned()
}

fn row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.weak(label);
        ui.label(value.into());
    });
}

fn infrastructure(ui: &mut egui::Ui, plugin: &Plugin) -> bool {
    if let Some(v) = &plugin.builtin {
        row(ui, tr("运行方式"), tr("服务端内置"));
        row(
            ui,
            tr("业务配置"),
            tr(if v.configured {
                "已配置"
            } else {
                "未配置"
            }),
        );
    }
    if let Some(v) = &plugin.storage {
        ui.weak(tr(
            "数据库为服务端必需组件，不能停用。后端由服务端启动配置选择，修改后需重启服务端。",
        ));
        row(ui, tr("数据库后端"), &v.backend);
        row(ui, tr("运行方式"), mode(&v.mode));
        row(ui, tr("健康状态"), tr(state(&v.runtime_state)));
    }
    if let Some(v) = &plugin.dsp {
        ui.strong(tr("处理链配置"));
        ui.label(tr(match v.chain_state.as_str() {
            "not_configured" => "未配置",
            "not_in_chain" => "已配置，未入链",
            "in_chain" => "已加入处理链",
            "disabled" => "已禁用",
            _ => "未知",
        }));
        ui.weak(tr("这里显示此插件在服务端启动配置中的处理链节点及原始位置，不表示当前歌曲正在执行这些处理。修改配置后需重启服务端。"));
        let mut nodes: Vec<_> = v.nodes.iter().collect();
        nodes.sort_by_key(|p| p.position);
        if nodes.is_empty() {
            ui.weak(tr("此 DSP 尚未加入处理链。"));
        }
        for node in nodes {
            ui.strong(format!(
                "{}. {} · {}",
                node.position,
                node.name,
                mode(&node.mode)
            ));
            row(ui, tr("节点版本"), node.version.to_string());
            let params = node
                .params
                .iter()
                .flatten()
                .map(|(key, v)| format!("{key} = {v}"))
                .collect::<Vec<_>>();
            row(
                ui,
                tr("参数"),
                if params.is_empty() {
                    tr("无参数").into()
                } else {
                    params.join(" · ")
                },
            );
        }
    }
    plugin.builtin.is_some() || plugin.storage.is_some() || plugin.dsp.is_some()
}

fn mode(value: &str) -> &str {
    tr(match value {
        "builtin" => "服务端内置",
        "process" => "独立进程",
        _ => "未知",
    })
}

fn state(value: &str) -> &str {
    match value {
        "ready" => "可用",
        "disabled" => "已禁用",
        "pending_configuration" => "待配置",
        "starting" => "正在启动",
        "draining" => "正在停止",
        "failed" => "启动或运行失败",
        "unavailable" => "不可用",
        "unknown" => "未知",
        _ => "暂不可用",
    }
}

fn field_name(value: &str) -> &str {
    match value {
        "lyrics" => "歌词",
        "artwork" => "专辑封面",
        "albumDetails" => "专辑详情",
        "albumTracks" => "专辑曲目",
        "artistDetails" => "歌手资料",
        "artistImage" => "歌手图像",
        "recordingIdentity" => "录音识别",
        "trackDetails" => "曲目资料",
        "workDetails" => "作品资料",
        other => other,
    }
}
