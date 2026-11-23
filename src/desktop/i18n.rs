use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    OnceLock,
};

const TAGS: [&str; 4] = ["zh-CN", "en", "zh-TW", "ja"];
static CURRENT: AtomicUsize = AtomicUsize::new(0);

fn supported(tag: &str) -> Option<usize> {
    let tag = tag.replace('_', "-").to_ascii_lowercase();
    match tag.split('-').next() {
        Some("zh")
            if tag.contains("hant")
                || (!tag.contains("hans")
                    && tag.split('-').any(|v| matches!(v, "tw" | "hk" | "mo"))) =>
        {
            Some(2)
        }
        Some("zh") => Some(0),
        Some("en") => Some(1),
        Some("ja") => Some(3),
        _ => None,
    }
}

pub fn apply(language: &str) {
    static SYSTEM: OnceLock<usize> = OnceLock::new();
    let current = if language == "system" {
        *SYSTEM.get_or_init(|| {
            sys_locale::get_locales()
                .find_map(|tag| supported(&tag))
                .unwrap_or(1)
        })
    } else {
        supported(language).unwrap_or(1)
    };
    CURRENT.store(current, Ordering::Relaxed);
}

pub fn locale() -> &'static str {
    TAGS[CURRENT.load(Ordering::Relaxed)]
}

pub fn language_tags() -> &'static [&'static str] {
    match locale() {
        "zh-TW" => &["zh-TW", "zh-Hant", "zh", "zh-HK", "zh-MO"],
        "en" => &["en", "en-US", "en-GB"],
        "ja" => &["ja", "ja-JP"],
        _ => &["zh-CN", "zh-Hans", "zh", "zh-SG"],
    }
}

type Catalog = HashMap<&'static str, [&'static str; 3]>;

fn messages() -> &'static Catalog {
    static MESSAGES: OnceLock<Catalog> = OnceLock::new();
    MESSAGES.get_or_init(|| {
        let mut data: Catalog = serde_json::from_str(include_str!("translations.json"))
            .expect("Invalid desktop language catalog");
        let settings: Catalog = serde_json::from_str(include_str!("settings_translations.json"))
            .expect("Invalid settings language catalog");
        data.extend(settings);
        let catalog: Catalog = serde_json::from_str(include_str!("catalog_translations.json"))
            .expect("Invalid catalog language catalog");
        data.extend(catalog);
        let shell: Catalog = serde_json::from_str(include_str!("shell_translations.json"))
            .expect("Invalid shell language catalog");
        data.extend(shell);
        data
    })
}

pub fn tr(key: &str) -> &str {
    let current = CURRENT.load(Ordering::Relaxed);
    if current == 0 {
        return key;
    }
    messages().get(key).map_or(key, |entry| entry[current - 1])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regional_tags_resolve_to_supported_languages() {
        assert_eq!(supported("zh_HK"), Some(2));
        assert_eq!(supported("zh-Hans-HK"), Some(0));
        assert_eq!(supported("ja-JP"), Some(3));
        assert_eq!(supported("fr-FR"), None);
        assert_eq!(messages()["语言"][0], "Language");
        assert_eq!(messages()["加入歌单"][1], "加入歌單");
        assert_eq!(messages()["桌面歌词"][2], "デスクトップ歌詞");
    }
}
