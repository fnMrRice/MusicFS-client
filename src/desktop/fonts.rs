use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};
use std::{fs, path::PathBuf};

// 加载系统的中文字形，保留 egui 的拉丁字形和符号作为原有字体链。
pub fn install(ctx: &Context) {
    #[cfg(target_os = "windows")]
    let candidates: Vec<PathBuf> = {
        let dir = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
            .join("Fonts");
        ["msyh.ttc", "msyh.ttf", "Deng.ttf", "simsun.ttc"]
            .iter()
            .map(|p| dir.join(p))
            .collect()
    };
    #[cfg(target_os = "macos")]
    let candidates: Vec<PathBuf> = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let candidates: Vec<PathBuf> = {
        let fontconfig = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", "sans-serif:lang=zh-cn"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .map(PathBuf::from);
        fontconfig
            .into_iter()
            .chain([
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into(),
                "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc".into(),
            ])
            .collect()
    };
    if let Some(bytes) = candidates.into_iter().find_map(|path| fs::read(path).ok()) {
        let mut fonts = FontDefinitions::default();
        let name = "MusicFS CJK".to_owned();
        fonts
            .font_data
            .insert(name.clone(), FontData::from_owned(bytes).into());
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
        ctx.set_fonts(fonts);
    }
}
