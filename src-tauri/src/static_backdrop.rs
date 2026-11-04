//! One cropped, blurred desktop image per opening. No files, uploads or live
//! capture loop; only the blurred PNG survives until the window is hidden.
use image::{codecs::png::PngEncoder, ImageEncoder, RgbaImage};
use tauri::Manager;
use windows_sys::Win32::{
    Graphics::{
        Dwm::DwmFlush,
        Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC,
            ReleaseDC, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HALFTONE, HBITMAP, HDC, HGDIOBJ,
            SRCCOPY,
        },
    },
    UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    },
};

const MAX_SIDE: u32 = 256;
const DOWNSAMPLE: f64 = 4.0;
const BLUR_CSS_PIXELS: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Area {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Area {
    fn intersect(self, bounds: Self) -> Option<Self> {
        let x = self.x.max(bounds.x);
        let y = self.y.max(bounds.y);
        let right = (self.x as i64 + self.width as i64).min(bounds.x as i64 + bounds.width as i64);
        let bottom =
            (self.y as i64 + self.height as i64).min(bounds.y as i64 + bounds.height as i64);
        (right > x as i64 && bottom > y as i64).then(|| Self {
            x,
            y,
            width: (right - x as i64) as i32,
            height: (bottom - y as i64) as i32,
        })
    }
}

fn dimensions(area: Area, scale: f64) -> (u32, u32) {
    let width = (area.width as f64 / scale / DOWNSAMPLE).max(1.0);
    let height = (area.height as f64 / scale / DOWNSAMPLE).max(1.0);
    let ratio = (MAX_SIDE as f64 / width.max(height)).min(1.0);
    (
        (width * ratio).round().max(1.0) as u32,
        (height * ratio).round().max(1.0) as u32,
    )
}

pub struct Snapshot {
    pub id: u32,
    area: Area,
    png: Vec<u8>,
}

impl Snapshot {
    pub fn png_for(&self, id: u32) -> Option<Vec<u8>> {
        (id == self.id).then(|| self.png.clone())
    }
}

struct ScreenDc(HDC);
impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(std::ptr::null_mut(), self.0);
        }
    }
}

struct Pixels {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    length: usize,
}

impl Pixels {
    fn new(source: HDC, width: u32, height: u32) -> Result<Self, String> {
        let mut pixels = Self {
            dc: std::ptr::null_mut(),
            bitmap: std::ptr::null_mut(),
            previous: std::ptr::null_mut(),
            bits: std::ptr::null_mut(),
            length: (width * height * 4) as usize,
        };
        unsafe {
            pixels.dc = CreateCompatibleDC(source);
            if pixels.dc.is_null() {
                return Err("无法准备背景取样".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            pixels.bitmap = CreateDIBSection(
                source,
                &info,
                DIB_RGB_COLORS,
                &mut pixels.bits as *mut *mut u8 as _,
                std::ptr::null_mut(),
                0,
            );
            if pixels.bitmap.is_null() || pixels.bits.is_null() {
                return Err("无法分配背景取样".into());
            }
            let previous = SelectObject(pixels.dc, pixels.bitmap);
            if previous.is_null() || previous as isize == -1 {
                return Err("无法选择背景取样".into());
            }
            pixels.previous = previous;
            // Initialize the complete buffer before asking GDI to fill it.
            std::ptr::write_bytes(pixels.bits, 0, pixels.length);
        }
        Ok(pixels)
    }

    fn copy_from(
        &self,
        source: HDC,
        area: Area,
        size: (u32, u32),
        layered: bool,
    ) -> Result<Vec<u8>, String> {
        unsafe {
            if SetStretchBltMode(self.dc, HALFTONE) == 0
                || SetBrushOrgEx(self.dc, 0, 0, std::ptr::null_mut()) == 0
            {
                return Err("背景取样缩放失败".into());
            }
            let operation = SRCCOPY | if layered { CAPTUREBLT } else { 0 };
            let copied = StretchBlt(
                self.dc,
                0,
                0,
                size.0 as i32,
                size.1 as i32,
                source,
                area.x,
                area.y,
                area.width,
                area.height,
                operation,
            );
            // Complete pending GDI writes even when the copy reports failure.
            let flushed = GdiFlush();
            if copied == 0 || flushed == 0 {
                return Err("桌面背景取样失败".into());
            }
            let mut rgba = std::slice::from_raw_parts(self.bits, self.length).to_vec();
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2); // Top-down GDI BGRA -> opaque RGBA.
                pixel[3] = 255;
            }
            Ok(rgba)
        }
    }
}

impl Drop for Pixels {
    fn drop(&mut self) {
        unsafe {
            // Never retain an unblurred sample in the capture buffer.
            if !self.bits.is_null() {
                std::ptr::write_bytes(self.bits, 0, self.length);
            }
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
        }
    }
}

fn blurred_png(mut unblurred: RgbaImage) -> Result<Vec<u8>, String> {
    let blurred = image::imageops::blur(&unblurred, BLUR_CSS_PIXELS / DOWNSAMPLE as f32);
    unblurred.as_mut().fill(0);
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            blurred.as_raw(),
            blurred.width(),
            blurred.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| "背景磨砂编码失败".to_owned())?;
    Ok(png)
}

fn area(window: &tauri::WebviewWindow) -> Result<(Area, f64), String> {
    let position = window.inner_position().map_err(|error| error.to_string())?;
    let size = window.inner_size().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    if !scale.is_finite() || scale <= 0.0 {
        return Err("背景取样缩放不可用".into());
    }
    let inset = 6.0 * scale;
    let left = inset.round() as i32;
    let top = inset.round() as i32;
    let card = Area {
        x: position.x + left,
        y: position.y + top,
        width: (size.width as f64 - inset).round() as i32 - left,
        height: (size.height as f64 - inset).round() as i32 - top,
    };
    let desktop = unsafe {
        Area {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
            height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    };
    card.intersect(desktop)
        .map(|area| (area, scale))
        .ok_or_else(|| "浮窗区域不在可取样桌面内".into())
}

pub fn prepare(window: &tauri::WebviewWindow, generation: u32) -> Result<Option<u32>, String> {
    let state = window.state::<crate::AppState>();
    let visible = window.is_visible().map_err(|error| error.to_string())?;
    if !visible {
        *state.static_backdrop.lock().map_err(|_| "背景状态不可用")? = None;
    }
    let (area, scale) = area(window)?;
    if visible {
        // Navigation/reload must not photograph the app itself. Keep the old
        // blurred image only while it still matches this location.
        let mut cached = state.static_backdrop.lock().map_err(|_| "背景状态不可用")?;
        if cached
            .as_ref()
            .is_some_and(|snapshot| snapshot.area != area)
        {
            *cached = None;
        }
        return Ok(cached.as_ref().map(|snapshot| snapshot.id));
    }
    let size = dimensions(area, scale);
    let screen = ScreenDc(unsafe { GetDC(std::ptr::null_mut()) });
    if screen.0.is_null() {
        return Err("桌面背景不可用".into());
    }
    unsafe {
        let _ = DwmFlush();
    } // Finish a previous hide before sampling.
    let pixels = Pixels::new(screen.0, size.0, size.1)?;
    let rgba = pixels.copy_from(screen.0, area, size, true)?;
    drop(pixels);
    drop(screen);
    let image = RgbaImage::from_raw(size.0, size.1, rgba).ok_or("背景取样尺寸无效")?;
    let png = blurred_png(image)?;
    *state.static_backdrop.lock().map_err(|_| "背景状态不可用")? = Some(Snapshot {
        id: generation,
        area,
        png,
    });
    Ok(Some(generation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_is_bounded_and_dpi_independent() {
        for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
            let area = Area {
                x: -200,
                y: 10,
                width: (744.0 * scale) as i32,
                height: (471.0 * scale) as i32,
            };
            let (width, height) = dimensions(area, scale);
            assert_eq!((width, height), (186, 118));
        }
        let size = dimensions(
            Area {
                x: 0,
                y: 0,
                width: 20_000,
                height: 10_000,
            },
            1.0,
        );
        assert!(size.0 <= MAX_SIDE && size.1 <= MAX_SIDE);
    }

    #[test]
    fn capture_area_handles_negative_monitor_coordinates_and_clips_edges() {
        let desktop = Area {
            x: -1920,
            y: 0,
            width: 3840,
            height: 1080,
        };
        let area = Area {
            x: -2000,
            y: 100,
            width: 744,
            height: 471,
        };
        assert_eq!(
            area.intersect(desktop),
            Some(Area {
                x: -1920,
                y: 100,
                width: 664,
                height: 471
            })
        );
        assert!(Area {
            x: 4000,
            y: 0,
            width: 744,
            height: 471
        }
        .intersect(desktop)
        .is_none());
    }

    #[test]
    fn blur_spreads_a_sharp_edge_and_keeps_the_image_opaque() {
        let mut image = RgbaImage::new(96, 64);
        for (x, _, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgba(if x < 48 {
                [0, 0, 0, 255]
            } else {
                [255, 255, 255, 255]
            });
        }
        let png = blurred_png(image).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (96, 64));
        assert!(decoded.get_pixel(43, 20)[0] > 0);
        assert!(decoded.get_pixel(52, 20)[0] < 255);
        assert!(decoded.pixels().all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn gdi_copy_converts_bgra_and_never_exposes_undefined_alpha() {
        // Synthetic memory DC only: this test does not read the desktop.
        let source = Pixels::new(std::ptr::null_mut(), 8, 8).unwrap();
        unsafe {
            let bytes = std::slice::from_raw_parts_mut(source.bits, source.length);
            for pixel in bytes.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[30, 20, 10, 0]);
            }
        }
        let destination = Pixels::new(source.dc, 8, 8).unwrap();
        let rgba = destination
            .copy_from(
                source.dc,
                Area {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                (8, 8),
                false,
            )
            .unwrap();
        assert!(rgba.chunks_exact(4).all(|pixel| pixel == [10, 20, 30, 255]));
    }

    #[test]
    fn only_the_matching_snapshot_can_be_loaded() {
        let snapshot = Snapshot {
            id: 7,
            area: Area {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            png: vec![1, 2, 3],
        };
        assert_eq!(snapshot.png_for(7), Some(vec![1, 2, 3]));
        assert_eq!(snapshot.png_for(6), None);
    }
}
