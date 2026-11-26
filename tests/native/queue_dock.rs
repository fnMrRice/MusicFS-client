use super::*;
use egui::{pos2, vec2, RawInput, ViewportId};

fn host() -> HostGeometry {
    HostGeometry {
        outer: Rect::from_min_max(pos2(100., 80.), pos2(1100., 800.)),
        client: pos2(108., 110.),
        home: pos2(1080., 680.),
        scale: 1.,
    }
}

fn child(x: f32, y: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(300., 240.))
}

#[test]
fn pin_uses_client_origin_and_physical_coordinates_across_monitors() {
    let ctx = Context::default();
    let mut raw = RawInput::default();
    let viewport = raw.viewports.get_mut(&ViewportId::ROOT).unwrap();
    viewport.native_pixels_per_point = Some(1.5);
    viewport.outer_rect = Some(Rect::from_min_size(pos2(-1008., 40.), vec2(908., 740.)));
    viewport.inner_rect = Some(Rect::from_min_size(pos2(-1000., 70.), vec2(892., 700.)));
    let popup = Rect::from_min_size(pos2(480., 200.), vec2(400., 360.));
    let _ = ctx.run(raw, |ctx| {
        let host = HostGeometry::read(ctx, popup.right_bottom()).unwrap();
        assert_eq!(host.popup_position(popup), pos2(-520., 270.));
        let mut dock = QueueDock::new();
        let actual = child(-900., 400.);
        let intended = dock
            .target(
                host,
                Rect::from_min_size(actual.min, popup.size() * 1.5),
                false,
            )
            .unwrap();
        assert_eq!(intended, pos2(-780., 405.));
        // 子窗口以不同 DPI 创建，原生位置命令必须传子窗口点坐标。
        assert_eq!(
            dock.move_if_needed(actual.min, intended, 2.),
            Some(pos2(-390., 202.5))
        );
        assert_eq!(dock.move_if_needed(actual.min, intended, 2.), None);
        assert_eq!(
            dock.move_if_needed(actual.min, intended, 1.5),
            Some(pos2(-520., 270.))
        );
        assert_eq!(dock.move_if_needed(intended, intended, 1.5), None);
    });
}

#[test]
fn dropping_against_inner_or_outer_edges_preserves_the_parallel_offset() {
    let main = host();
    for (dropped, aligned) in [
        (child(1108., 280.), pos2(1100., 280.)),
        (child(793., 280.), pos2(800., 280.)),
        (child(-195., 280.), pos2(-200., 280.)),
        (child(106., 280.), pos2(100., 280.)),
        (child(380., -157.), pos2(380., -160.)),
        (child(380., 88.), pos2(380., 80.)),
        (child(380., 565.), pos2(380., 560.)),
        (child(380., 810.), pos2(380., 800.)),
        (child(1104., 803.), pos2(1100., 800.)),
    ] {
        let mut dock = QueueDock::new();
        dock.begin_drag();
        assert_eq!(dock.target(main, dropped, true), None);
        assert_eq!(dock.target(main, dropped, false), Some(aligned));
        let offset = vec2(-340., 189.);
        let moved = HostGeometry {
            outer: main.outer.translate(offset),
            ..main
        };
        assert_eq!(dock.target(moved, dropped, false), Some(aligned + offset));
    }
}

#[test]
fn edges_follow_resize_and_the_popup_corner_can_be_reattached() {
    let main = host();
    let right = child(1095., 420.);
    let bigger = HostGeometry {
        outer: Rect::from_min_max(main.outer.min, pos2(1500., 950.)),
        home: pos2(1480., 830.),
        ..main
    };
    let mut dock = QueueDock::new();
    dock.begin_drag();
    assert_eq!(dock.target(main, right, false), Some(pos2(1100., 420.)));
    assert_eq!(dock.target(bigger, right, false), Some(pos2(1500., 570.)));
    dock.begin_drag();
    assert_eq!(dock.target(main, child(840., 410.), false), None);
    assert_eq!(dock.target(bigger, child(840., 410.), false), None);
    dock.begin_drag();
    let near_popup = child(788., 444.);
    assert_eq!(dock.target(main, near_popup, false), Some(pos2(780., 440.)));
    assert_eq!(
        dock.target(bigger, near_popup, false),
        Some(pos2(1180., 590.))
    );
}

#[test]
fn distant_edge_extensions_and_a_free_window_stay_independent() {
    let main = host();
    let distant = child(1096., -550.);
    let mut dock = QueueDock::new();
    dock.begin_drag();
    assert_eq!(dock.target(main, distant, false), None);
    let moved = HostGeometry {
        outer: main.outer.translate(vec2(0., -500.)),
        ..main
    };
    assert_eq!(dock.target(moved, distant, false), None);
}
