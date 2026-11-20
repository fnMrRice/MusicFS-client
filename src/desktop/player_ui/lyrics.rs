use std::time::Duration;

pub enum Lyrics {
    Empty,
    Plain(String),
    Timed(Vec<Line>),
}

pub struct Line {
    pub time_ms: u64,
    pub text: String,
}

fn timestamp(tag: &str) -> Option<u64> {
    let (minutes, seconds) = tag.split_once(':')?;
    if minutes.is_empty() || minutes.len() > 3 || !minutes.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (seconds, fraction) = seconds.split_once(['.', ':']).unwrap_or((seconds, ""));
    if seconds.len() != 2
        || !seconds.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let seconds: u64 = seconds.parse().ok()?;
    if seconds >= 60 {
        return None;
    }
    let ms = fraction.parse::<u64>().unwrap_or(0) * 10u64.pow((3 - fraction.len()) as u32);
    Some((minutes.parse::<u64>().ok()? * 60 + seconds) * 1000 + ms)
}

impl Lyrics {
    pub fn parse(raw: Option<String>) -> Self {
        let Some(raw) = raw.filter(|text| !text.trim().is_empty()) else {
            return Self::Empty;
        };
        let offset = raw
            .lines()
            .find_map(|line| {
                let tag = line
                    .trim()
                    .trim_start_matches('\u{feff}')
                    .strip_prefix('[')?
                    .strip_suffix(']')?;
                let (key, value) = tag.split_once(':')?;
                key.eq_ignore_ascii_case("offset")
                    .then(|| value.trim().parse::<i64>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        let mut values = Vec::new();
        for line in raw.lines() {
            let mut text = line.trim().trim_start_matches('\u{feff}');
            let mut times = Vec::new();
            while let Some((tag, rest)) = text.strip_prefix('[').and_then(|s| s.split_once(']')) {
                let Some(ms) = timestamp(tag) else { break };
                times.push(ms.saturating_add_signed(offset.saturating_neg()));
                text = rest;
            }
            values.extend(times.into_iter().map(|time_ms| Line {
                time_ms,
                text: text.trim().to_owned(),
            }));
        }
        if values.is_empty() {
            return Self::Plain(raw);
        }
        values.sort_by_key(|line| line.time_ms);
        let mut lines: Vec<Line> = Vec::new();
        for value in values {
            if let Some(previous) = lines
                .last_mut()
                .filter(|line| line.time_ms == value.time_ms)
            {
                if !value.text.is_empty() {
                    if !previous.text.is_empty() {
                        previous.text.push('\n');
                    }
                    previous.text.push_str(&value.text);
                }
            } else {
                lines.push(value);
            }
        }
        Self::Timed(lines)
    }
}

pub fn time(ms: u64) -> String {
    let seconds = Duration::from_millis(ms).as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offsets_repeated_tags_and_translations_use_the_output_clock() {
        let source = "[offset:+250]\r\n[ar:Example]\n[00:01.5][00:05.050]First\n[00:01.500]Translation\n[00:03:20]";
        let Lyrics::Timed(lines) = Lyrics::parse(Some(source.into())) else {
            panic!("Expected timed lyrics")
        };
        assert_eq!(
            lines.iter().map(|l| l.time_ms).collect::<Vec<_>>(),
            [1250, 2950, 4800]
        );
        assert_eq!(lines[0].text, "First\nTranslation");
        assert_eq!(lines[1].text, "");
        assert_eq!(lines.partition_point(|l| l.time_ms <= 1250), 1);
    }
    #[test]
    fn untimed_text_and_missing_lyrics_are_distinct() {
        assert!(matches!(
            Lyrics::parse(Some("[00:99]Verse".into())),
            Lyrics::Plain(_)
        ));
        assert!(matches!(Lyrics::parse(None), Lyrics::Empty));
    }
}
