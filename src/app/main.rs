#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../desktop/browse/mod.rs"]
mod browse;
#[path = "../desktop/connection.rs"]
mod connection;
#[path = "../desktop/fonts.rs"]
mod fonts;
#[path = "../desktop/i18n.rs"]
mod i18n;
#[path = "../desktop/model.rs"]
mod model;
#[path = "../desktop/player_ui/mod.rs"]
mod player_ui;
#[path = "../desktop/task.rs"]
mod task;
#[path = "../desktop/ui/mod.rs"]
mod ui;

use eframe::egui;
use std::time::Duration;

struct MusicFs {
    state: model::DesktopState,
    ui: ui::DesktopUi,
}

impl eframe::App for MusicFs {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.state.update(ctx);
        self.ui.show(ctx, &mut self.state);
    }

    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.state.shutdown();
    }
}

fn main() -> eframe::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Could not start the MusicFS runtime");
    let handle = runtime.handle().clone();
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("icons/128x128.png"))
        .expect("Could not read the MusicFS icon");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("io.github.fnMrRice.musicfs.desktop")
            .with_inner_size([1232.0, 800.0])
            .with_min_inner_size([660.0, 500.0])
            .with_icon(icon),
        centered: true,
        ..Default::default()
    };
    let result = eframe::run_native(
        "MusicFS",
        options,
        Box::new(move |creation| {
            fonts::install(&creation.egui_ctx);
            Ok(Box::new(MusicFs {
                state: model::DesktopState::new(handle),
                ui: ui::DesktopUi::default(),
            }))
        }),
    );
    runtime.shutdown_timeout(Duration::from_secs(2));
    result
}
