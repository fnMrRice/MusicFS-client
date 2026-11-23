use super::*;

// 用真正的 HICON 在预填背景的内存 DC 上绘制，校验 Windows 的 Alpha 混合。
fn draw(handle: HICON, w: i32, h: i32, background: Rgb) -> Vec<u8> {
    let (canvas, bits) = dib(w, h).unwrap();
    let len = (w * h * 4) as usize;
    let [r, g, b] = background;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits, len) };
    for p in pixels.chunks_exact_mut(4) {
        p.copy_from_slice(&[b, g, r, 255]);
    }
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        assert!(!dc.is_null());
        let old = SelectObject(dc, canvas.0);
        assert!(!old.is_null());
        let ok = DrawIconEx(dc, 0, 0, handle, w, h, 0, null_mut(), DI_NORMAL);
        GdiFlush();
        SelectObject(dc, old);
        DeleteDC(dc);
        assert_ne!(ok, 0);
    }
    pixels.to_vec()
}

#[test]
fn thumbnail_icons_match_system_dpi_and_support_native_alpha_compositing() {
    // 包含标准的浅色与深色任务栏，以及带颜色的高对比度按钮配色。
    let palettes = [
        (
            Colors {
                ink: [27; 3],
                edge: [246; 3],
            },
            [244; 3],
        ),
        (
            Colors {
                ink: [247; 3],
                edge: [35; 3],
            },
            [32; 3],
        ),
        (
            Colors {
                ink: [245, 229, 43],
                edge: [0, 51, 74],
            },
            [0, 51, 74],
        ),
    ];
    for dpi in [96, 144, 192] {
        let system = Appearance::current(dpi);
        let (w, h) = (system.width, system.height);
        assert!(w >= 16 && h >= 16);
        let glyphs = Glyphs::new(dpi).unwrap();
        assert!(!glyphs.needs_refresh(dpi));
        for name in ["previous", "play", "pause", "next"] {
            let mut info = ICONINFO::default();
            assert_ne!(unsafe { GetIconInfo(glyphs.get(name), &mut info) }, 0);
            let (color, _mask) = (Bitmap(info.hbmColor), Bitmap(info.hbmMask));
            let mut bmp = BITMAP::default();
            assert_ne!(
                unsafe {
                    GetObjectW(
                        color.0,
                        size_of::<BITMAP>() as i32,
                        (&mut bmp as *mut BITMAP).cast(),
                    )
                },
                0
            );
            assert_eq!((bmp.bmWidth, bmp.bmHeight, bmp.bmBitsPixel), (w, h, 32));
        }
        for (colors, bg) in palettes {
            for symbol in SYMBOLS {
                let handle = icon(symbol, Appearance { colors, ..system }).unwrap();
                let actual = draw(handle.0, w, h, bg);
                let (input, _) = raster(symbol, w as usize, h as usize, colors);
                let mut partial = 0;
                let mut solid_ink = 0;
                for (src, dst) in input.chunks_exact(4).zip(actual.chunks_exact(4)) {
                    let a = u32::from(src[3]);
                    for c in 0..3 {
                        // 透明边缘叠加时通道只乘一次 Alpha，过滤黑边或发白的锯齿。
                        let expected =
                            (u32::from(src[c]) * a + u32::from(bg[2 - c]) * (255 - a) + 127) / 255;
                        assert!(expected.abs_diff(u32::from(dst[c])) <= 1, "DPI={dpi}, input={src:?}, background={bg:?}, expected={expected}, actual={dst:?}");
                    }
                    partial += usize::from(a > 0 && a < 255);
                    let [r, g, b] = colors.ink;
                    solid_ink += usize::from(dst[..3] == [b, g, r]);
                }
                assert!(partial > 0);
                assert!(solid_ink > (w * h) as usize / 9);
                assert_eq!(&actual[..3], &[bg[2], bg[1], bg[0]]);
            }
        }
    }
}
