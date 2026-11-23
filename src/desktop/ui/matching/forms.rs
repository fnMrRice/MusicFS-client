use super::{decision::Decision, evidence::Evidence, Services};
use crate::i18n::tr;
use eframe::egui;

pub(super) fn review(
    ui: &mut egui::Ui,
    svc: Services<'_>,
    lookup: &dyn Fn(&str) -> String,
    action: &Decision,
    items: &[Evidence],
    enabled: bool,
    playback_error: &mut Option<String>,
) -> bool {
    let (help, button) = match action {
        Decision::Unlink(_) => (
            "解除后，此来源将在曲目列表中单独显示。原文件不会改变。",
            "确认解除",
        ),
        Decision::Exclude { excluded: true, .. } => (
            "这两个来源将排除在同一录音的匹配和批量合并之外。",
            "确认排除",
        ),
        Decision::Exclude {
            excluded: false, ..
        } => (
            "撤销排除后，服务端可以再次把这两个来源列为候选。",
            "撤销排除",
        ),
        _ => (
            "将所选歌曲关联为同一录音，在曲目列表中合成一行。所有来源和原文件都会保留。",
            "确认合并",
        ),
    };
    ui.label(tr(help));
    let valid =
        !matches!(action, Decision::Link { sources, .. } if !(2..=256).contains(&sources.len()));
    if !valid {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr(if items.len() > 256 {
                "一次最多合并 256 个来源版本，请减少选择。"
            } else {
                "至少选择两个不同的来源版本。"
            }),
        );
    }
    for source in items {
        source.show(ui, svc, playback_error, lookup);
    }
    ui.add_enabled(valid && enabled, egui::Button::new(tr(button)))
        .clicked()
}

pub(super) fn bulk(
    ui: &mut egui::Ui,
    by_score: &mut bool,
    minimum: &mut String,
    preview: &mut bool,
    enabled: bool,
) -> Option<Decision> {
    ui.label(tr("处理全部来源中你可见的当前可用曲目，不受当前来源筛选、已加载页或所选曲目限制。所有来源和原文件都会保留。"));
    if *preview {
        ui.strong(tr(if *by_score {
            "按分数合并"
        } else {
            "合并重名重作者"
        }));
        if *by_score {
            ui.label(format!("{}  {minimum}", tr("最低分数（0–100）")));
        }
    } else {
        ui.add_enabled_ui(enabled, |ui| {
            ui.radio_value(by_score, true, tr("按分数合并"));
            if *by_score {
                ui.horizontal_wrapped(|ui| {
                    let label = ui.label(tr("最低分数（0–100）"));
                    ui.add(egui::TextEdit::singleline(minimum).desired_width(60.0))
                        .labelled_by(label.id);
                });
            }
            ui.radio_value(by_score, false, tr("合并重名重作者"));
        });
    }
    ui.weak(tr(if *by_score {
        "合并评分达到最低分数的待确认候选。评分是核对优先级，不是相同概率；请先完成重复识别。"
    } else {
        "按规范化后的完整曲名和作者集合相同合并，无需先生成候选；曲名或作者为空的曲目不合并。"
    }));
    ui.weak(tr("已排除、存在录音冲突或超过每组 256 个来源版本的分组会跳过。同名同作者仍可能是不同录音，请按需要选择规则。"));
    ui.weak(tr("手动解除关联的曲目不会被再次自动合并。"));
    let score = minimum.trim().parse::<u8>().ok().filter(|n| *n <= 100);
    let valid = !*by_score || score.is_some();
    if !valid {
        ui.colored_label(
            ui.visuals().error_fg_color,
            tr("最低分数必须是 0 到 100 的整数。"),
        );
    }
    let label = tr(if *preview {
        "开始合并"
    } else {
        "预览合并规则"
    });
    if ui
        .add_enabled(enabled && valid, egui::Button::new(label))
        .clicked()
    {
        if *preview {
            return Some(Decision::Bulk {
                score: if *by_score { score } else { None },
            });
        }
        *preview = true;
    }
    if *preview && enabled && ui.button(tr("更改规则")).clicked() {
        *preview = false;
    }
    None
}

pub(super) enum Dialog {
    Candidates,
    Review {
        action: Decision,
        items: Vec<Evidence>,
        revision: Option<u64>,
        from_candidates: bool,
    },
    Bulk {
        by_score: bool,
        minimum: String,
        preview: bool,
    },
}

impl Dialog {
    pub fn title(&self) -> &str {
        tr(match self {
            Self::Candidates => "重复识别",
            Self::Review {
                action: Decision::Unlink(_),
                ..
            } => "解除关联",
            Self::Review {
                action: Decision::Exclude { excluded: true, .. },
                ..
            } => "不是同一录音",
            Self::Review {
                action:
                    Decision::Exclude {
                        excluded: false, ..
                    },
                ..
            } => "撤销排除",
            Self::Review { .. } => "合并歌曲",
            Self::Bulk { .. } => "一键合并重复音乐",
        })
    }
}

pub(super) fn merge(items: Vec<Evidence>, revision: Option<u64>, from_candidates: bool) -> Dialog {
    let sources = items.iter().map(|s| s.source_track_id.clone()).collect();
    let target = items.iter().find_map(|s| s.recording_id.clone());
    Dialog::Review {
        action: Decision::Link { sources, target },
        items,
        revision,
        from_candidates,
    }
}
pub(super) fn exclusion(
    items: Vec<Evidence>,
    revision: Option<u64>,
    excluded: bool,
    from_candidates: bool,
) -> Dialog {
    let action = Decision::Exclude {
        left: items[0].source_track_id.clone(),
        right: items[1].source_track_id.clone(),
        excluded,
    };
    Dialog::Review {
        action,
        items,
        revision,
        from_candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::accesskit::{Action, ActionRequest, NodeId};

    #[test]
    fn preview_requires_a_separate_confirmation_before_mutating() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut score = true;
        let mut minimum = "90".to_owned();
        let mut preview = false;
        let mut render = |click: Option<NodeId>, next: &str| {
            let mut input = egui::RawInput::default();
            if let Some(target) = click {
                input
                    .events
                    .push(egui::Event::AccessKitActionRequest(ActionRequest {
                        action: Action::Click,
                        target,
                        data: None,
                    }));
            }
            let mut request = None;
            let output = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    request = bulk(ui, &mut score, &mut minimum, &mut preview, true);
                });
            });
            let node = output.platform_output.accesskit_update.and_then(|tree| {
                tree.nodes
                    .into_iter()
                    .find(|(_, node)| node.label() == Some(tr(next)))
                    .map(|(id, _)| id)
            });
            (request, node)
        };
        let (_, button) = render(None, "预览合并规则");
        assert!(render(Some(button.expect("Preview button")), "开始合并")
            .0
            .is_none());
        let (_, confirm) = render(None, "开始合并");
        assert!(matches!(
            render(Some(confirm.expect("Confirm button")), "").0,
            Some(Decision::Bulk { score: Some(90) })
        ));
    }
}
