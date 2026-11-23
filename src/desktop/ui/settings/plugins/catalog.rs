use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Translations = BTreeMap<String, String>;

pub(super) fn localized<'a>(fallback: &'a str, values: &'a Translations) -> &'a str {
    let choices: &[&str] = match crate::i18n::locale() {
        "zh-TW" => &["zh-TW", "zh-Hant", "zh", "zh-HK", "zh-MO"],
        "en" => &["en", "en-US", "en-GB"],
        "ja" => &["ja", "ja-JP"],
        _ => &["zh-CN", "zh-Hans", "zh", "zh-SG"],
    };
    choices
        .iter()
        .find_map(|tag| {
            values.iter().find(|(k, v)| {
                k.replace('_', "-").eq_ignore_ascii_case(tag) && !v.trim().is_empty()
            })
        })
        .map_or(fallback, |(_, name)| name)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    pub revision: String,
    pub writable: bool,
    pub restart_required: bool,
    pub items: Vec<Plugin>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct Identity {
    pub name: String,
    pub localized_names: Translations,
    pub features: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Plugin {
    pub plugin_id: String,
    pub plugin: Option<Identity>,
    pub version: String,
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub role: String,
    pub display_count: Option<Count>,
    pub supports_multiple_instances: bool,
    pub configured_enabled: bool,
    pub state_mutable: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub settings_available: bool,
    pub storage: Option<Storage>,
    pub dsp: Option<Dsp>,
    pub builtin: Option<Builtin>,
    pub execution: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Instance {
    pub instance_id: String,
    pub role: String,
    pub capabilities: Option<Vec<String>>,
    pub owner_user_id: Option<String>,
    pub account_name: Option<String>,
    pub configured_enabled: bool,
    pub runtime_state: String,
    pub reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Count {
    pub value: u64,
    pub unit: String,
    #[serde(default)]
    pub localized_units: Translations,
}

#[derive(Deserialize)]
pub(super) struct Builtin {
    pub configured: bool,
    pub capabilities: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Storage {
    pub backend: String,
    pub mode: String,
    pub runtime_state: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Dsp {
    pub chain_state: String,
    pub nodes: Vec<Node>,
}

#[derive(Deserialize)]
pub(super) struct Node {
    pub position: usize,
    pub name: String,
    pub version: usize,
    pub mode: String,
    pub params: Option<BTreeMap<String, f64>>,
}

#[derive(Deserialize, Default)]
pub(super) struct Online {
    pub items: Vec<OnlineInstance>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OnlineInstance {
    pub instance_id: String,
    pub name: Option<String>,
    pub account_id: Option<String>,
}

impl Plugin {
    pub fn name(&self) -> &str {
        self.plugin
            .as_ref()
            .map_or(crate::i18n::tr("未知来源"), |p| {
                localized(&p.name, &p.localized_names)
            })
    }

    pub fn purposes(&self) -> BTreeSet<&str> {
        let mut result: BTreeSet<_> = self
            .plugin
            .iter()
            .flat_map(|p| &p.features)
            .map(String::as_str)
            .collect();
        if self.role == "scraper" || self.instances.iter().any(|p| p.role == "scraper") {
            result.insert("scraper");
        }
        if result.is_empty() {
            result.insert("");
        }
        result
    }

    pub fn matches(&self, purposes: &BTreeSet<String>, text: &str) -> bool {
        (purposes.is_empty() || self.purposes().iter().any(|p| purposes.contains(*p)))
            && (self.plugin_id.to_lowercase().contains(text)
                || self.plugin.iter().any(|p| {
                    std::iter::once(&p.name)
                        .chain(p.localized_names.values())
                        .any(|s| s.to_lowercase().contains(text))
                }))
    }
}

pub(super) fn purpose(value: &str) -> &str {
    match value {
        "" => "未分类",
        "music-source" => "音乐源",
        "content-type" => "内容类型",
        "audio-codec" => "音频编解码",
        "audio-processing" => "音频处理",
        "audio-dsp" => "DSP",
        "audio-analysis" => "音频分析",
        "metadata" => "元数据",
        "scraper" => "刮削插件",
        "database" => "数据库",
        "audio-decryption" => "音频解密",
        "decode" => "音频解码",
        other => other,
    }
}
