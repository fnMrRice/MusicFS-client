use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        HiDpi::GetSystemMetricsForDpi,
        WindowsAndMessaging::*,
    },
};

type Rgb = [u8; 3];

#[derive(Clone, Copy, PartialEq)]
struct Colors {
    ink: Rgb,
    edge: Rgb,
}

fn system_color(index: SYS_COLOR_INDEX) -> Rgb {
    let [r, g, b, _] = unsafe { GetSysColor(index) }.to_le_bytes();
    [r, g, b]
}

impl Colors {
    fn shell() -> Self {
        let mut contrast = HIGHCONTRASTW {
            cbSize: size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        let enabled = unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.cbSize,
                (&mut contrast as *mut HIGHCONTRASTW).cast(),
                0,
            )
        } != 0;
        if enabled && contrast.dwFlags & HCF_HIGHCONTRASTON != 0 {
            return Self {
                ink: system_color(COLOR_BTNTEXT),
                edge: system_color(COLOR_BTNFACE),
            };
        }
        // Windows 允许任务栏和应用使用不同主题。只读取当前用户的 Shell 个性化设置。
        let mut light = 0u32;
        let mut bytes = size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                windows_sys::w!(
                    "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
                ),
                windows_sys::w!("SystemUsesLightTheme"),
                RRF_RT_REG_DWORD,
                null_mut(),
                (&mut light as *mut u32).cast(),
                &mut bytes,
            )
        };
        if status == 0 && bytes == size_of::<u32>() as u32 {
            if light == 0 {
                Self {
                    ink: [247; 3],
                    edge: [35; 3],
                }
            } else {
                Self {
                    ink: [27; 3],
                    edge: [246; 3],
                }
            }
        } else {
            Self {
                ink: system_color(COLOR_BTNTEXT),
                edge: system_color(COLOR_BTNFACE),
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Appearance {
    width: i32,
    height: i32,
    colors: Colors,
}

impl Appearance {
    fn current(dpi: u32) -> Self {
        let dpi = if dpi == 0 { 96 } else { dpi };
        Self {
            width: unsafe { GetSystemMetricsForDpi(SM_CXICON, dpi) },
            height: unsafe { GetSystemMetricsForDpi(SM_CYICON, dpi) },
            colors: Colors::shell(),
        }
    }
}

#[derive(Clone, Copy)]
enum Symbol {
    Previous,
    Play,
    Pause,
    Next,
}
const SYMBOLS: [Symbol; 4] = [Symbol::Previous, Symbol::Play, Symbol::Pause, Symbol::Next];

impl Symbol {
    // 使用 24 单位画布的实心轮廓。边缘的异色细线使符号在有强调色的任务栏上也能辨认。
    fn distance(self, x: f32, y: f32) -> f32 {
        fn triangle(x: f32, y: f32, left: f32, tip: f32, height: f32) -> f32 {
            let slope = height / (tip - left);
            (x - left).min(((tip - x) * slope - (y - 12.).abs()) / (1. + slope * slope).sqrt())
        }
        fn bar(x: f32, y: f32, left: f32, right: f32) -> f32 {
            (x - left).min(right - x).min(y - 4.).min(20. - y)
        }
        match self {
            Self::Play => triangle(x, y, 6., 20., 8.5),
            Self::Pause => bar(x, y, 5.5, 10.).max(bar(x, y, 14., 18.5)),
            Self::Previous | Self::Next => {
                let x = if matches!(self, Self::Previous) {
                    24. - x
                } else {
                    x
                };
                triangle(x, y, 3.5, 15.5, 8.).max(bar(x, y, 17.5, 20.5))
            }
        }
    }
}

struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}

struct Icon(HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.0);
        }
    }
}

// 给 CreateIconIndirect 的 32 位顶向下 DIB 保留直通 BGRA；由系统在绘制时合成透明度。
fn raster(symbol: Symbol, w: usize, h: usize, colors: Colors) -> (Vec<u8>, Vec<u8>) {
    let stride = w.div_ceil(16) * 2;
    let mut mask = vec![0u8; stride * h];
    let mut pixels = Vec::with_capacity(w * h * 4);
    let scale = 24. / w.min(h) as f32;
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u16; 4];
            for v in 0..4 {
                for u in 0..4 {
                    let sx = 12. + (x as f32 - w as f32 / 2. + (u as f32 + 0.5) / 4.) * scale;
                    let sy = 12. + (y as f32 - h as f32 / 2. + (v as f32 + 0.5) / 4.) * scale;
                    let d = symbol.distance(sx, sy);
                    let rgb = if d >= 0. {
                        colors.ink
                    } else if d >= -0.75 {
                        colors.edge
                    } else {
                        continue;
                    };
                    sum[0] += u16::from(rgb[2]);
                    sum[1] += u16::from(rgb[1]);
                    sum[2] += u16::from(rgb[0]);
                    sum[3] += 1;
                }
            }
            if sum[3] == 0 {
                mask[y * stride + x / 8] |= 0x80 >> (x % 8);
                pixels.extend([0; 4]);
            } else {
                let count = sum[3];
                for value in &sum[..3] {
                    pixels.push(((value + count / 2) / count) as u8);
                }
                pixels.push(((255 * count + 8) / 16) as u8);
            }
        }
    }
    (pixels, mask)
}

fn dib(w: i32, h: i32) -> Result<(Bitmap, *mut u8), String> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = null_mut();
    let handle =
        unsafe { CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let bitmap = Bitmap(handle);
    if bits.is_null() {
        return Err("No thumbnail bitmap storage.".into());
    }
    Ok((bitmap, bits.cast()))
}

fn icon(symbol: Symbol, geometry: Appearance) -> Result<Icon, String> {
    let (w, h) = (geometry.width, geometry.height);
    let (pixels, mask) = raster(symbol, w as usize, h as usize, geometry.colors);
    let (color, bits) = dib(w, h)?;
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits, pixels.len());
    }
    let mono = unsafe { CreateBitmap(w, h, 1, 1, mask.as_ptr().cast()) };
    if mono.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mask = Bitmap(mono);
    let info = ICONINFO {
        fIcon: 1,
        hbmColor: color.0,
        hbmMask: mask.0,
        ..Default::default()
    };
    let handle = unsafe { CreateIconIndirect(&info) };
    if handle.is_null() {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(Icon(handle))
    }
}

pub(super) struct Glyphs {
    appearance: Appearance,
    icons: [Icon; 4],
}
impl Glyphs {
    pub(super) fn new(dpi: u32) -> Result<Self, String> {
        let appearance = Appearance::current(dpi);
        if !(1..=512).contains(&appearance.width) || !(1..=512).contains(&appearance.height) {
            return Err("Invalid Windows thumbnail icon size.".into());
        }
        // 每个 HICON 单独持有，即使创建后续符号失败也能释放前面已创建的资源。
        let [previous, play, pause, next] = SYMBOLS;
        Ok(Self {
            appearance,
            icons: [
                icon(previous, appearance)?,
                icon(play, appearance)?,
                icon(pause, appearance)?,
                icon(next, appearance)?,
            ],
        })
    }
    pub(super) fn needs_refresh(&self, dpi: u32) -> bool {
        self.appearance != Appearance::current(dpi)
    }
    pub(super) fn get(&self, name: &str) -> HICON {
        match name {
            "previous" => self.icons[0].0,
            "play" => self.icons[1].0,
            "pause" => self.icons[2].0,
            "next" => self.icons[3].0,
            _ => null_mut(),
        }
    }
}

#[cfg(test)]
#[path = "thumbnail_icon_tests.rs"]
mod tests;
