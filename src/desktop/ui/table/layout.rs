use eframe::egui;

pub const HEIGHT: f32 = 66.;
pub const HEADER: f32 = 44.;

pub struct Columns {
    pub check: f32,
    pub number: f32,
    pub title: f32,
    pub artist: f32,
    pub album: f32,
    pub provider: f32,
    pub format: f32,
    pub duration: f32,
    pub actions: f32,
}

impl Columns {
    pub fn new(width: f32, show_provider: bool) -> Self {
        Self::for_table(width, show_provider, true)
    }

    pub fn online(width: f32) -> Self {
        Self::for_table(width, true, false)
    }

    fn for_table(width: f32, show_provider: bool, catalog: bool) -> Self {
        let check = if !catalog {
            0.
        } else if width >= 680. {
            40.
        } else {
            30.
        };
        let number = if width >= 680. { 33. } else { 25. };
        let actions = if !catalog {
            0.
        } else if width >= 810. {
            (width * 0.205).clamp(166., 265.)
        } else if width >= 615. {
            90.
        } else {
            43.
        };
        let duration = if width >= 810. {
            (width * 0.088).clamp(66., 103.)
        } else if width >= 600. {
            57.
        } else {
            48.
        };
        let format = if catalog && width >= 720. {
            (width * 0.078).clamp(57., 90.)
        } else {
            0.
        };
        let provider = if show_provider && width >= 910. {
            (width * 0.105).min(130.)
        } else {
            0.
        };
        let flex = (width - check - number - duration - format - actions - provider).max(0.);
        let wide = ((width - 960.) / 320.).clamp(0., 1.);
        let artist = if width >= 535. {
            flex * (0.245 + 0.085 * wide)
        } else {
            0.
        };
        let album = if width >= 700. {
            flex * (0.215 + 0.105 * wide)
        } else {
            0.
        };
        Self {
            check,
            number,
            title: (flex - artist - album).max(0.),
            artist,
            album,
            provider,
            format,
            duration,
            actions,
        }
    }
    pub fn cells(&self, row: egui::Rect) -> [egui::Rect; 9] {
        let mut x = row.left();
        [
            self.check,
            self.number,
            self.title,
            self.artist,
            self.album,
            self.provider,
            self.format,
            self.duration,
            self.actions,
        ]
        .map(|w| {
            let r =
                egui::Rect::from_min_size(egui::pos2(x, row.top()), egui::vec2(w, row.height()));
            x += w;
            r
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    Track(usize),
    Source(usize, usize),
    More,
}

pub struct Lines {
    expanded: Vec<(usize, usize)>,
    tracks: usize,
    pub len: usize,
}
impl Lines {
    pub fn new(tracks: usize, expanded: Vec<(usize, usize)>, more: bool) -> Self {
        let len = tracks + expanded.iter().map(|(_, n)| n).sum::<usize>() + usize::from(more);
        Self {
            tracks,
            expanded,
            len,
        }
    }
    // 展开来源仍保持虚拟滚动。只处理展开的条目，无需遍历全部曲目构造行数组。
    pub fn at(&self, slot: usize) -> Line {
        let mut added = 0;
        for &(i, n) in &self.expanded {
            let index = i + added;
            if slot <= index {
                break;
            }
            if slot <= index + n {
                return Line::Source(i, slot - index - 1);
            }
            added += n;
        }
        if slot - added < self.tracks {
            Line::Track(slot - added)
        } else {
            Line::More
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_sources_keep_display_order_and_page_control() {
        let lines = Lines::new(4, vec![(0, 2), (2, 1)], true);
        assert_eq!(
            (0..lines.len).map(|i| lines.at(i)).collect::<Vec<_>>(),
            vec![
                Line::Track(0),
                Line::Source(0, 0),
                Line::Source(0, 1),
                Line::Track(1),
                Line::Track(2),
                Line::Source(2, 0),
                Line::Track(3),
                Line::More
            ]
        );
    }
    #[test]
    fn columns_fit_the_visible_table_at_each_breakpoint() {
        for width in [320., 500., 660., 720., 810., 900., 1092., 1500.] {
            for c in [
                Columns::new(width, false),
                Columns::new(width, true),
                Columns::online(width),
            ] {
                let row = egui::Rect::from_min_size(egui::pos2(10., 0.), egui::vec2(width, HEIGHT));
                assert!((c.cells(row)[8].right() - row.right()).abs() < 0.01);
                assert!(c.title >= 100.);
            }
            let online = Columns::online(width);
            assert_eq!((online.check, online.format, online.actions), (0., 0., 0.));
            assert!(online.title > Columns::new(width, true).title);
        }
    }
}
