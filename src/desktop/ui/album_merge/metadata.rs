use crate::i18n::tr;
use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
pub(super) struct Alias {
    pub language: String,
    pub title: String,
}
#[derive(Deserialize)]
struct Name {
    name: String,
}
#[derive(Deserialize)]
struct Album {
    #[serde(rename = "albumId")]
    id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Item {
    pub track_id: String,
    pub title: String,
    pub revision: u64,
    #[serde(default)]
    localized_titles: Vec<Alias>,
    artists: Vec<Name>,
    albums: Vec<Album>,
    album_ids: Option<Vec<String>>,
    genre: Option<String>,
    year: Option<u16>,
    rating: Option<u8>,
    cover_path: Option<String>,
    selected_source: Option<Value>,
}
impl Item {
    pub fn in_album(&self, id: &str) -> bool {
        match &self.album_ids {
            Some(ids) => ids.iter().any(|album_id| album_id == id),
            None => self.albums.iter().any(|a| a.id == id),
        }
    }
    pub fn versions(&self) -> usize {
        self.selected_source.as_ref().map_or(0, |s| {
            s["versions"].as_array().map_or(1, |a| a.len().max(1))
        })
    }
    pub fn draft(&self) -> Draft {
        Draft {
            title: self.title.clone(),
            aliases: self.localized_titles.clone(),
            artists: self
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            genre: self.genre.clone().unwrap_or_default(),
            year: self.year.map(|n| n.to_string()).unwrap_or_default(),
            rating: self.rating.map(|n| n.to_string()).unwrap_or_default(),
            cover: self.cover_path.clone().unwrap_or_default(),
        }
    }
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Draft {
    pub title: String,
    pub aliases: Vec<Alias>,
    artists: String,
    genre: String,
    year: String,
    rating: String,
    cover: String,
}
#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum Field {
    Title,
    Artists,
    Genre,
    Year,
    Rating,
    Cover,
    Aliases,
}
pub(super) const FIELDS: [Field; 7] = [
    Field::Title,
    Field::Artists,
    Field::Genre,
    Field::Year,
    Field::Rating,
    Field::Cover,
    Field::Aliases,
];
impl Field {
    pub fn label(self) -> &'static str {
        tr(match self {
            Self::Title => "曲名",
            Self::Artists => "歌手",
            Self::Genre => "音乐风格",
            Self::Year => "年份",
            Self::Rating => "评分",
            Self::Cover => "封面路径",
            Self::Aliases => "多语言曲名",
        })
    }
}
impl Draft {
    pub fn auto(items: &[Item], target: usize, display_title: Option<&str>) -> Self {
        let primary = &items[target];
        let mut result = primary.draft();
        result.aliases.clear();
        let mut artists = Vec::new();
        let title = display_title
            .unwrap_or(&primary.title)
            .trim()
            .to_lowercase();
        for t in std::iter::once(primary).chain(
            items
                .iter()
                .enumerate()
                .filter_map(|(i, t)| (i != target).then_some(t)),
        ) {
            for a in &t.artists {
                if !artists.contains(&a.name.as_str()) {
                    artists.push(a.name.as_str());
                }
            }
            let incoming = t.draft();
            for field in [Field::Genre, Field::Year, Field::Rating, Field::Cover] {
                if result.preview(field).is_empty() {
                    result.assign(field, &incoming);
                }
            }
            for a in &t.localized_titles {
                if !result.aliases.iter().any(|b| {
                    a.language.trim().eq_ignore_ascii_case(b.language.trim())
                        && fold(&a.title) == fold(&b.title)
                }) {
                    result.aliases.push(a.clone());
                }
            }
            if fold(&t.title) != title
                && !result
                    .aliases
                    .iter()
                    .any(|a| fold(&a.title) == fold(&t.title))
            {
                result.aliases.push(Alias {
                    language: "und".into(),
                    title: t.title.clone(),
                });
            }
        }
        result.artists = artists.join("\n");
        result
    }
    fn scalar(&mut self, field: Field) -> &mut String {
        match field {
            Field::Title => &mut self.title,
            Field::Artists => &mut self.artists,
            Field::Genre => &mut self.genre,
            Field::Year => &mut self.year,
            Field::Rating => &mut self.rating,
            Field::Cover => &mut self.cover,
            Field::Aliases => unreachable!(),
        }
    }
    pub fn assign(&mut self, field: Field, from: &Draft) {
        match field {
            Field::Title => self.title.clone_from(&from.title),
            Field::Artists => self.artists.clone_from(&from.artists),
            Field::Genre => self.genre.clone_from(&from.genre),
            Field::Year => self.year.clone_from(&from.year),
            Field::Rating => self.rating.clone_from(&from.rating),
            Field::Cover => self.cover.clone_from(&from.cover),
            Field::Aliases => self.aliases.clone_from(&from.aliases),
        }
    }
    pub fn preview(&self, field: Field) -> String {
        match field {
            Field::Title => self.title.clone(),
            Field::Artists => self.artists.replace('\n', " / "),
            Field::Genre => self.genre.clone(),
            Field::Year => self.year.clone(),
            Field::Rating => self.rating.clone(),
            Field::Cover => self.cover.clone(),
            Field::Aliases => self
                .aliases
                .iter()
                .map(|a| format!("{} ({})", a.title, a.language))
                .collect::<Vec<_>>()
                .join(" / "),
        }
    }
    pub fn edit(&mut self, ui: &mut egui::Ui, field: Field) -> bool {
        if field == Field::Aliases {
            let mut changed = false;
            let mut remove = None;
            for (i, row) in self.aliases.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut row.language)
                                    .desired_width(70.)
                                    .hint_text(tr("语言标签")),
                            )
                            .changed();
                        changed |= ui
                            .add(egui::TextEdit::singleline(&mut row.title).hint_text(tr("别名")))
                            .changed();
                        if ui.small_button(tr("移除")).clicked() {
                            remove = Some(i);
                        }
                    })
                });
            }
            if let Some(i) = remove {
                self.aliases.remove(i);
                changed = true;
            }
            if ui
                .add_enabled(self.aliases.len() < 64, egui::Button::new(tr("添加别名")))
                .clicked()
            {
                self.aliases.push(Alias::default());
                changed = true;
            }
            return changed;
        }
        let value = self.scalar(field);
        if field == Field::Artists {
            return ui
                .add(
                    egui::TextEdit::multiline(value)
                        .desired_rows(2)
                        .hint_text(tr("每行填写一位歌手。")),
                )
                .changed();
        }
        if field == Field::Rating {
            let prev = value.clone();
            egui::ComboBox::from_id_salt("star-rating")
                .selected_text(if value.is_empty() {
                    tr("未评分")
                } else {
                    value.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(value, String::new(), tr("未评分"));
                    for i in 1..=5 {
                        ui.selectable_value(value, i.to_string(), i.to_string());
                    }
                });
            return prev != *value;
        }
        ui.text_edit_singleline(value).changed()
    }
    pub fn wire(&self) -> Result<Value, &'static str> {
        let line = |s: &str, limit| s.chars().count() <= limit && !s.chars().any(char::is_control);
        let title = self.title.trim();
        if title.is_empty() || !line(title, 256) {
            return Err("请输入不超过 256 个字符的单行曲名。");
        }
        let artists: Vec<_> = self
            .artists
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if artists.len() > 128
            || artists.iter().any(|a| !line(a, 256))
            || artists.iter().copied().collect::<HashSet<_>>().len() != artists.len()
        {
            return Err("最多可填写 128 位歌手，每个名称最多 256 个字符，且不能重复。");
        }
        let mut seen = HashSet::new();
        let mut aliases = Vec::new();
        for a in &self.aliases {
            let (tag, name) = (a.language.trim(), a.title.trim());
            if self.aliases.len() > 64
                || !valid_tag(tag)
                || name.is_empty()
                || !line(name, 256)
                || !seen.insert((tag.to_ascii_lowercase(), fold(name)))
            {
                return Err("请填写有效的语言标签和名称，同一语言下的名称不能重复。");
            }
            aliases.push(json!({"language":tag,"title":name}));
        }
        if !line(self.genre.trim(), 128) {
            return Err("音乐风格最多 128 个字符。");
        }
        if !line(self.cover.trim(), 4096) {
            return Err("封面路径最多 4096 个字符。");
        }
        let year = num(&self.year, 9999).ok_or("请输入 1 到 9999 之间的整数年份。")?;
        let rating = num(&self.rating, 5).ok_or("评分范围为 1 至 5。")?;
        Ok(
            json!({"title":title,"localizedTitles":aliases,"artistNames":artists,"year":year,"rating":rating,
            "genre":nonempty(&self.genre),"coverPath":nonempty(&self.cover)}),
        )
    }
}
fn fold(s: &str) -> String {
    s.trim().to_lowercase()
}
fn nonempty(s: &str) -> Option<&str> {
    Some(s.trim()).filter(|s| !s.is_empty())
}
fn num(s: &str, max: u16) -> Option<Option<u16>> {
    let Some(s) = nonempty(s) else {
        return Some(None);
    };
    if !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok().filter(|x| (1..=max).contains(x)).map(Some)
}
fn valid_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    let first = parts.next().unwrap_or_default();
    !s.is_empty()
        && s.len() <= 63
        && first.bytes().all(|c| c.is_ascii_alphabetic())
        && ((2..=8).contains(&first.len())
            || (first.eq_ignore_ascii_case("x") || first.eq_ignore_ascii_case("i"))
                && s.contains('-'))
        && parts
            .all(|p| !p.is_empty() && p.len() <= 8 && p.bytes().all(|c| c.is_ascii_alphanumeric()))
}
