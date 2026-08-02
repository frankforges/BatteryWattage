use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Render a wattage number as an HICON suitable for Shell_NotifyIcon.
/// `text_scale` is a percentage (e.g. 100 = normal, 150 = 50% larger).
pub fn make_icon(
    label: &str,
    color: [u8; 4],
    show_background: bool,
    text_scale: i32,
) -> Result<HICON> {
    const ICON_SIZE: i32 = 64;
    let scale = text_scale as f64 / 100.0;

    // Choose font size based on character count, scaled by text_scale
    let base_font = match label.len() {
        0..=2 => 42.0,
        3 => 34.0,
        4 => 28.0,
        _ => 22.0,
    };
    let font_size = (base_font * scale).round() as i32;

    // ── Create ICON_SIZE × ICON_SIZE 32bpp BGRA DIB ──────────────
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: ICON_SIZE,
            biHeight: -ICON_SIZE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default(); 1],
    };

    let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
    let hdc_screen = unsafe { GetDC(None) };
    let hbm = unsafe { CreateDIBSection(hdc_screen, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) };
    unsafe { ReleaseDC(None, hdc_screen) };

    let hbm = hbm?;
    let pixel_bytes = (ICON_SIZE * ICON_SIZE * 4) as usize;

    unsafe { core::ptr::write_bytes(bits, 0u8, pixel_bytes) };

    // ── Memory DC ─────────────────────────────────────────────────
    let hdc_mem = unsafe { CreateCompatibleDC(None) };
    if hdc_mem.is_invalid() {
        unsafe {
            let _ = DeleteObject(hbm);
        };
        return Err(Error::new(E_FAIL, "CreateCompatibleDC failed"));
    }
    let old_bm = unsafe { SelectObject(hdc_mem, hbm) };

    // ── Background pill ────────────────────────────────────────────
    if show_background {
        let (tx, ty, tw, th) = text_bounds(label, font_size, ICON_SIZE);
        let pad_x = (4.0 * scale).round() as i32;
        let pad_y = (2.0 * scale).round() as i32;
        let radius = (8.0 * scale).round() as i32;
        draw_rounded_rect(
            bits,
            ICON_SIZE,
            (tx - pad_x, ty - pad_y, tx + tw + pad_x, ty + th + pad_y),
            radius,
            [0, 0, 0, 210],
        );
    }

    // ── Font ──────────────────────────────────────────────────────
    let face: Vec<u16> = "Consolas\0".encode_utf16().collect();
    let font = unsafe {
        CreateFontW(
            font_size,
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            DEFAULT_QUALITY.0 as u32,
            FF_DONTCARE.0 as u32,
            PCWSTR::from_raw(face.as_ptr()),
        )
    };

    let old_font = unsafe { SelectObject(hdc_mem, font) };
    unsafe { SetBkMode(hdc_mem, TRANSPARENT) };

    let text_rgb = COLORREF((color[2] as u32) << 16 | (color[1] as u32) << 8 | color[0] as u32);
    unsafe { SetTextColor(hdc_mem, text_rgb) };

    // ── Draw text ─────────────────────────────────────────────────
    let rect = RECT {
        left: 0,
        top: 0,
        right: ICON_SIZE,
        bottom: ICON_SIZE,
    };
    let mut wide_label: Vec<u16> = label.encode_utf16().collect();
    let dt_flags = DRAW_TEXT_FORMAT(DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_NOCLIP.0);
    let mut rect_mut = rect;
    unsafe {
        DrawTextW(hdc_mem, &mut wide_label, &mut rect_mut, dt_flags);
    }

    unsafe {
        let _ = GdiFlush();
    };

    // ── Restore GDI ───────────────────────────────────────────────
    unsafe {
        SelectObject(hdc_mem, old_font);
    };
    unsafe {
        let _ = DeleteObject(font);
    };
    unsafe {
        SelectObject(hdc_mem, old_bm);
    };
    unsafe {
        let _ = DeleteDC(hdc_mem);
    };

    // ── Post-process alpha ────────────────────────────────────────
    let pixels = unsafe { core::slice::from_raw_parts_mut(bits as *mut u8, pixel_bytes) };
    for px in pixels.chunks_exact_mut(4) {
        if px[3] == 0 && (px[2] != 0 || px[1] != 0 || px[0] != 0) {
            px[3] = 255;
        }
    }

    // ── Mask bitmap (1bpp) ────────────────────────────────────────
    let total_pixels = ICON_SIZE as usize * ICON_SIZE as usize;
    let mask_bytes = total_pixels.div_ceil(8);
    let mut mask_bits: Vec<u8> = vec![0u8; mask_bytes];
    for y in 0..ICON_SIZE as usize {
        for x in 0..ICON_SIZE as usize {
            let idx = y * ICON_SIZE as usize + x;
            if pixels[idx * 4 + 3] > 0 {
                let bit_pos = y * ICON_SIZE as usize + x;
                mask_bits[bit_pos / 8] |= 1u8 << (7 - (bit_pos % 8));
            }
        }
    }

    let hbm_mask = unsafe {
        CreateBitmap(
            ICON_SIZE,
            ICON_SIZE,
            1,
            1,
            Some(mask_bits.as_ptr() as *const _),
        )
    };

    // ── Build icon ────────────────────────────────────────────────
    let icon_info = ICONINFO {
        fIcon: BOOL::from(true),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: hbm_mask,
        hbmColor: hbm,
    };

    let hicon = unsafe { CreateIconIndirect(&icon_info) };
    unsafe {
        let _ = DeleteObject(hbm_mask);
        let _ = DeleteObject(hbm);
    };
    hicon
}

/// Estimate bounding rectangle for text centered in the icon.
/// Returns (left, top, width, height).
fn text_bounds(label: &str, font_size: i32, canvas: i32) -> (i32, i32, i32, i32) {
    let char_width = (font_size as f64 * 0.6) as i32;
    let text_width = char_width * label.len() as i32;
    let text_height = font_size;
    let tx = (canvas - text_width) / 2;
    let ty = (canvas - text_height) / 2;
    (tx, ty, text_width, text_height)
}

/// Draw a filled rounded rectangle directly into a BGRA pixel buffer.
fn draw_rounded_rect(
    bits: *mut core::ffi::c_void,
    stride_pixels: i32,
    bounds: (i32, i32, i32, i32),
    radius: i32,
    color: [u8; 4],
) {
    let (left, top, right, bottom) = bounds;
    let [b, g, r, a] = color;
    let pixels = unsafe {
        core::slice::from_raw_parts_mut(
            bits as *mut u8,
            (stride_pixels * stride_pixels * 4) as usize,
        )
    };
    for y in top.max(0)..bottom.min(stride_pixels) {
        for x in left.max(0)..right.min(stride_pixels) {
            if rounded_rect_contains(x, y, left, top, right, bottom, radius) {
                let idx = (y as usize * stride_pixels as usize + x as usize) * 4;
                pixels[idx] = b;
                pixels[idx + 1] = g;
                pixels[idx + 2] = r;
                pixels[idx + 3] = a;
            }
        }
    }
}

fn rounded_rect_contains(
    px: i32,
    py: i32,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
) -> bool {
    let r = radius;
    if px >= left + r && px < right - r {
        return py >= top && py < bottom;
    }
    if py >= top + r && py < bottom - r {
        return px >= left && px < right;
    }
    for (cx, cy) in [
        (left + r, top + r),
        (right - r - 1, top + r),
        (left + r, bottom - r - 1),
        (right - r - 1, bottom - r - 1),
    ] {
        if (px - cx) * (px - cx) + (py - cy) * (py - cy) <= r * r {
            return true;
        }
    }
    false
}
