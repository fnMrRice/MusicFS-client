use eframe::egui::{Context, FontData, FontDefinitions, FontFamily, FontId, Id};
use std::{fs, path::PathBuf};

const BOLD: &str = "MusicFS Semibold";

fn candidates(bold: bool) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let directory =
            PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
                .join("Fonts");
        let files = if bold {
            ["msyhbd.ttc", "msyhbd.ttf", "Dengb.ttf"]
        } else {
            ["msyh.ttc", "msyh.ttf", "Deng.ttf"]
        };
        files.map(|name| directory.join(name)).into()
    }
    #[cfg(target_os = "macos")]
    {
        if bold {
            vec!["/System/Library/Fonts/STHeiti Medium.ttc".into()]
        } else {
            [
                "/System/Library/Fonts/PingFang.ttc",
                "/System/Library/Fonts/STHeiti Light.ttc",
            ]
            .map(PathBuf::from)
            .into()
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let pattern = if bold {
            "sans-serif:lang=zh-cn:style=Bold"
        } else {
            "sans-serif:lang=zh-cn"
        };
        let from_system = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", pattern])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .map(PathBuf::from);
        let noto = if bold {
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"
        } else {
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"
        };
        from_system.into_iter().chain([noto.into()]).collect()
    }
}

pub fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let mut load = |bold, name: &str| {
        let bytes = candidates(bold)
            .into_iter()
            .find_map(|path| fs::read(path).ok())?;
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_owned(bytes).into());
        Some(name.to_owned())
    };
    let body = load(false, "MusicFS UI");
    let heading = load(true, BOLD);
    let regular = fonts.families.entry(FontFamily::Proportional).or_default();
    // 同一系统字体负责拉丁及中日韩正文，使用 egui 的后备字体补足符号。
    if let Some(name) = &body {
        regular.insert(0, name.clone());
    }
    if let Some(name) = heading {
        let mut stack = regular.clone();
        stack.insert(0, name);
        fonts.families.insert(FontFamily::Name(BOLD.into()), stack);
        ctx.data_mut(|d| d.insert_temp(Id::new(BOLD), true));
    }
    if let Some(name) = body {
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .push(name);
    }
    ctx.set_fonts(fonts);
}

pub fn bold(ctx: &Context, size: f32) -> FontId {
    if ctx.data(|d| d.get_temp::<bool>(Id::new(BOLD)).unwrap_or_default()) {
        FontId::new(size, FontFamily::Name(BOLD.into()))
    } else {
        FontId::proportional(size)
    }
}
