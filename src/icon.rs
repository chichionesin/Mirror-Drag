//! The tray icon, drawn procedurally so the binary needs no resources or image files:
//! a rounded blue square with a white "|—|" glyph (two edges and the span between them).

/// Supersampling factor per axis for anti-aliased edges.
const SUPERSAMPLE: usize = 4;
const BLUE: [f32; 3] = [0.16, 0.47, 0.84];
const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

/// Renders a `size`×`size` icon as top-down rows of premultiplied 32-bit BGRA pixels
/// (`0xAARRGGBB` as an integer), the layout GDI expects for a 32-bpp DIB.
pub fn render(size: usize) -> Vec<u32> {
    let samples = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    let mut pixels = Vec::with_capacity(size * size);
    for py in 0..size {
        for px in 0..size {
            let mut sum = [0f32; 4]; // r, g, b, coverage
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let x = (px as f32 + (sx as f32 + 0.5) / SUPERSAMPLE as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SUPERSAMPLE as f32) / size as f32;
                    if let Some([r, g, b]) = color_at(x, y) {
                        sum[0] += r;
                        sum[1] += g;
                        sum[2] += b;
                        sum[3] += 1.0;
                    }
                }
            }
            // Averaging the color sums over all samples yields premultiplied values directly.
            let channel = |v: f32| (v / samples * 255.0).round() as u32;
            pixels.push(
                channel(sum[3]) << 24
                    | channel(sum[0]) << 16
                    | channel(sum[1]) << 8
                    | channel(sum[2]),
            );
        }
    }
    pixels
}

/// Color at normalized coordinates (0..1); `None` is transparent.
fn color_at(x: f32, y: f32) -> Option<[f32; 3]> {
    let within = |x0: f32, x1: f32, y0: f32, y1: f32| x >= x0 && x < x1 && y >= y0 && y < y1;
    let left_edge = within(0.18, 0.32, 0.24, 0.76);
    let right_edge = within(0.68, 0.82, 0.24, 0.76);
    let span = within(0.32, 0.68, 0.44, 0.56);
    if left_edge || right_edge || span {
        return Some(WHITE);
    }
    in_rounded_square(x, y, 0.03, 0.22).then_some(BLUE)
}

fn in_rounded_square(x: f32, y: f32, inset: f32, radius: f32) -> bool {
    let (lo, hi) = (inset, 1.0 - inset);
    if x < lo || x >= hi || y < lo || y >= hi {
        return false;
    }
    let cx = x.clamp(lo + radius, hi - radius);
    let cy = y.clamp(lo + radius, hi - radius);
    (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
}

#[cfg(windows)]
pub use win::create_icon;

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::{copy_nonoverlapping, null_mut};

    use windows_sys::Win32::Graphics::Gdi::{
        CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateIconIndirect, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON,
    };

    /// Builds an `HICON` at the system's small-icon size. The caller owns it (`DestroyIcon`);
    /// null on failure.
    pub fn create_icon() -> HICON {
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16);
        let pixels = super::render(size as usize);
        unsafe {
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size, // negative: top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..zeroed()
            };
            let mut bits: *mut c_void = null_mut();
            let color =
                CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if color.is_null() || bits.is_null() {
                return null_mut();
            }
            copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, pixels.len());
            // 1-bpp AND mask, all zero (rows are word-aligned): visibility comes from alpha.
            let mask_bytes = vec![0u8; (size as usize).div_ceil(16) * 2 * size as usize];
            let mask = CreateBitmap(size, size, 1, 1, mask_bytes.as_ptr() as *const c_void);
            let icon = CreateIconIndirect(&ICONINFO {
                fIcon: 1,
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: color,
            });
            DeleteObject(mask);
            DeleteObject(color);
            icon
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_expected_layout() {
        let size = 32;
        let pixels = render(size);
        assert_eq!(pixels.len(), size * size);
        let at = |x: usize, y: usize| pixels[y * size + x];

        assert_eq!(at(0, 0) >> 24, 0, "corners are transparent");
        assert_eq!(at(16, 16), 0xFFFF_FFFF, "the span is opaque white");
        assert_eq!(at(8, 16), 0xFFFF_FFFF, "the left edge bar is opaque white");
        let background = at(16, 4);
        assert_eq!(background >> 24, 0xFF, "background is opaque");
        assert!(
            background & 0xFF > (background >> 16) & 0xFF,
            "background is blue"
        );
        assert_eq!(at(3, 3), at(28, 28), "icon is symmetric");
    }

    #[test]
    fn premultiplied_alpha_never_exceeds_coverage() {
        for pixel in render(16) {
            let alpha = pixel >> 24;
            for shift in [16, 8, 0] {
                assert!((pixel >> shift) & 0xFF <= alpha);
            }
        }
    }
}
