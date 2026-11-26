use crate::{
    i18n::tr,
    icons,
    ui::style::{self, ComboBoxMotion},
};
use eframe::egui;
use musicfs_server_adapter::Track;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Order {
    #[default]
    Original,
    Title,
    Artist,
    Album,
}

impl Order {
    fn label(self) -> &'static str {
        tr(match self {
            Self::Original => "默认顺序",
            Self::Title => "曲名",
            Self::Artist => "歌手",
            Self::Album => "专辑",
        })
    }
}

pub(crate) struct Metadata<'a> {
    pub title: &'a str,
    pub artists: &'a [String],
    pub album: Option<&'a str>,
}

impl<'a> From<&'a Track> for Metadata<'a> {
    fn from(track: &'a Track) -> Self {
        Self {
            title: &track.title,
            artists: &track.artists,
            album: track.album.as_deref(),
        }
    }
}

#[derive(Default)]
pub(crate) struct PlaylistTools {
    text: String,
    terms: Vec<String>,
    order: Order,
}

impl PlaylistTools {
    pub fn active(&self) -> bool {
        !self.terms.is_empty() || self.order != Order::Original
    }

    pub fn searching(&self) -> bool {
        !self.terms.is_empty()
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> bool {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let previous = self.order;
            egui::ComboBox::from_id_salt("playlist-track-sort")
                .width(108.)
                .selected_text(self.order.label())
                .show_animated(ui, |ui| {
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                        for order in [Order::Original, Order::Title, Order::Artist, Order::Album] {
                            style::selectable_value(ui, &mut self.order, order, order.label());
                        }
                    });
                });
            let clear =
                !self.text.is_empty() && icons::button(ui, "close", tr("清除"), 28., 14.).clicked();
            if clear {
                self.text.clear();
            }
            let field = ui.add_sized(
                [190., 36.],
                egui::TextEdit::singleline(&mut self.text)
                    .id(egui::Id::new("active-playlist-track-query"))
                    .hint_text(tr("搜索当前列表"))
                    .margin(egui::Margin::symmetric(12, 4))
                    .vertical_align(egui::Align::Center)
                    .char_limit(256),
            );
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                field.request_focus();
            }
            if field.changed() || clear {
                self.terms = self
                    .text
                    .split_whitespace()
                    .map(str::to_lowercase)
                    .collect();
                true
            } else {
                previous != self.order
            }
        })
        .inner
    }

    pub fn matches(&self, meta: &Metadata<'_>) -> bool {
        if self.terms.is_empty() {
            return true;
        }
        let text = format!(
            "{}\n{}\n{}",
            meta.title,
            meta.artists.join(" / "),
            meta.album.unwrap_or_default()
        )
        .to_lowercase();
        self.terms.iter().all(|term| text.contains(term))
    }

    pub fn sorted(&self) -> bool {
        self.order != Order::Original
    }

    pub fn sort_key(&self, meta: Metadata<'_>) -> String {
        match self.order {
            Order::Artist => meta.artists.join(" / ").to_lowercase(),
            Order::Album => meta.album.unwrap_or_default().to_lowercase(),
            _ => meta.title.to_lowercase(),
        }
    }
}

#[derive(Default)]
pub(super) struct LocalTracks {
    pub tools: PlaylistTools,
    // 普通浏览直接使用表格数据。启用筛选或排序后保留原顺序，清除时无需重新读取快照。
    original: Option<Vec<Track>>,
}

impl LocalTracks {
    pub fn reset_pages(&mut self) {
        self.original = None;
    }

    pub fn rebuild(&mut self, visible: &mut Vec<Track>) {
        if !self.tools.active() {
            if let Some(original) = self.original.take() {
                *visible = original;
            }
            return;
        }
        let original = self.original.get_or_insert_with(|| std::mem::take(visible));
        visible.clear();
        visible.extend(
            original
                .iter()
                .filter(|t| self.tools.matches(&Metadata::from(*t)))
                .cloned(),
        );
        self.sort(visible);
    }

    pub fn append(&mut self, page: Vec<Track>, visible: &mut Vec<Track>) {
        if !self.tools.active() {
            visible.extend(page);
            return;
        }
        visible.extend(
            page.iter()
                .filter(|t| self.tools.matches(&Metadata::from(*t)))
                .cloned(),
        );
        self.original.get_or_insert_with(Vec::new).extend(page);
        self.sort(visible);
    }

    fn sort(&self, visible: &mut [Track]) {
        if self.tools.sorted() {
            visible.sort_by_cached_key(|track| self.tools.sort_key(Metadata::from(track)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorting_filtered_snapshot_rows_preserves_sources_and_original_order() {
        let row = |id: &str, title: &str, source: &str| Track {
            id: id.into(),
            title: title.into(),
            artists: vec!["Band".into()],
            album: Some("Studio".into()),
            forced_source_track_id: Some(source.into()),
            ..Default::default()
        };
        let mut view = LocalTracks::default();
        let mut rows = vec![
            row("first", "Song", "auto"),
            row("second", "Overture", "local"),
        ];
        view.tools.terms = vec!["band".into(), "studio".into()];
        view.tools.order = Order::Title;
        view.rebuild(&mut rows);
        view.append(
            vec![row("first", "Song", "cloud"), row("last", "Anthem", "disc")],
            &mut rows,
        );
        let ids = |rows: &[Track]| {
            rows.iter()
                .map(|r| r.forced_source_track_id.clone().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&rows), ["disc", "local", "auto", "cloud"]);
        view.tools.terms = vec!["song".into()];
        view.rebuild(&mut rows);
        assert_eq!(ids(&rows), ["auto", "cloud"]);
        view.tools = Default::default();
        view.rebuild(&mut rows);
        assert_eq!(ids(&rows), ["auto", "local", "cloud", "disc"]);
    }
}
