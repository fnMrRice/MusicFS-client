use crate::i18n::tr;
use eframe::egui::Ui;
use musicfs_server_adapter::tasks::{LibraryTask, TaskDetails};

pub(super) fn task_details(ui: &mut Ui, task: &LibraryTask) {
    let finished = matches!(task.status.as_str(), "succeeded" | "failed" | "cancelled");
    match &task.details {
        TaskDetails::MetadataRefresh {
            provider_name,
            roots,
        } => {
            ui.strong(provider_name);
            ui.label(
                roots
                    .iter()
                    .map(|root| root.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" / "),
            );
        }
        TaskDetails::AlbumAutoScrape { progress, result } => {
            ui.strong(tr("专辑自动刮削"));
            if progress.total > 0 {
                ui.label(
                    tr("已处理 {0}/{1} 张专辑")
                        .replace("{0}", &progress.completed.to_string())
                        .replace("{1}", &progress.total.to_string()),
                );
            }
            if finished {
                ui.label(
                    tr("已保存 {0}，已保留 {1}，未匹配 {2}，失败 {3}")
                        .replace("{0}", &result.updated.to_string())
                        .replace("{1}", &result.retained.to_string())
                        .replace("{2}", &result.not_found.to_string())
                        .replace("{3}", &result.failed.to_string()),
                );
            }
        }
        TaskDetails::BulkMerge { result } => {
            ui.strong(tr("一键合并重复音乐"));
            if finished {
                ui.label(
                    tr("已合并 {0} 组，涉及 {1} 个来源，跳过 {2} 组")
                        .replace("{0}", &result.merged_groups.to_string())
                        .replace("{1}", &result.merged_sources.to_string())
                        .replace("{2}", &result.skipped_groups.to_string()),
                );
            }
        }
    }
    if let Some(error) = &task.error {
        ui.colored_label(ui.visuals().error_fg_color, &error.message);
    }
}
