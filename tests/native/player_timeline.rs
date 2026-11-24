use super::*;
use egui::{Event, Modifiers, PointerButton};

#[test]
fn playback_bar_uses_session_codec_bitrate_and_device_channels() {
    let flac = PlaybackFormat {
        codec: Some("flac".into()),
        sample_rate: Some(44100),
        bitrate_bps: Some(717577),
    };
    let channels = tr("{0} 声道").replace("{0}", "2");
    assert_eq!(
        format_label(Some(&flac), Some(2)),
        format!("FLAC · 44.1 kHz · 717.6 kbps · {channels}")
    );
    let pcm = PlaybackFormat {
        codec: Some("pcm_s16le".into()),
        sample_rate: None,
        bitrate_bps: None,
    };
    assert_eq!(format_label(Some(&pcm), None), "PCM");
    assert_eq!(format_label(None, Some(2)), tr("暂无编码信息"));
}

#[test]
fn playback_ticks_never_seek_but_a_pointer_drag_commits_the_draft_once() {
    let ctx = egui::Context::default();
    let mut player = PlayerUi::default();
    player.snapshot.duration_ms = Some(90_000);
    player.snapshot.seekable = true;
    player.snapshot.seek_range_ms = Some((0, 90_000));
    let mut frame = |position, events| {
        player.snapshot.position_ms = position;
        let mut result = None;
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600., 105.));
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let (bar, _) = ui.allocate_exact_size(screen.size(), egui::Sense::hover());
                        result = player.timeline(ui, bar);
                    });
            },
        );
        result
    };
    let start = egui::pos2(103., 6.);
    let end = egui::pos2(202., 6.);
    let button = |pos, pressed| Event::PointerButton {
        pos,
        pressed,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
    };
    for ms in [0, 167, 321, 503] {
        assert_eq!(frame(ms, vec![]), None);
    }
    assert_eq!(
        frame(664, vec![Event::PointerMoved(start), button(start, true)]),
        None
    );
    assert_eq!(frame(824, vec![Event::PointerMoved(end)]), None);
    let position = frame(995, vec![button(end, false)]).expect("Drag release must seek once");
    assert!((29_000..=31_000).contains(&position));
    assert_eq!(frame(position + 177, vec![]), None);
}
