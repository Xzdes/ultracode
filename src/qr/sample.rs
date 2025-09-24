// Семплинг QR v1 (21×21) с корректной геометрией всего символа.
// Ключевые идеи:
// - Строим векторы модуля ux=(TR-TL)/14, uy=(BL-TL)/14.
// - По ним получаем 4 внешних угла символа (координаты модулей 0..20).
// - Гомография из [0..1]^2 всей матрицы в эти 4 угла (никакой экстраполяции).
// - Лёгкая автокалибровка: анизотропные масштабы su/sv и сдвиги du/dv (в норм. коорд).
// - Суперсэмплинг 3×3; скоринг по центральному участку таймингов (8..=12).
//
// Логи: углы, длины |ux|/|uy|, выбранные su/sv/du/dv, тайминги, 8×8 превью.

use super::{finder::{self, PointF}, QrOptions};
use crate::prelude::GrayImage;
use super::data::N1;

#[inline]
fn sample_bilinear(img: &GrayImage<'_>, x: f32, y: f32) -> u8 {
    let w = (img.width as i32 - 1).max(0);
    let h = (img.height as i32 - 1).max(0);

    let xf = x.clamp(0.0, w as f32);
    let yf = y.clamp(0.0, h as f32);

    let x0 = xf.floor() as i32;
    let y0 = yf.floor() as i32;
    let x1 = (x0 + 1).clamp(0, w);
    let y1 = (y0 + 1).clamp(0, h);

    let dx = xf - x0 as f32;
    let dy = yf - y0 as f32;

    let idx = |xx: i32, yy: i32| -> usize { (yy as usize) * img.width + (xx as usize) };
    let p00 = img.data[idx(x0, y0)] as f32;
    let p10 = img.data[idx(x1, y0)] as f32;
    let p01 = img.data[idx(x0, y1)] as f32;
    let p11 = img.data[idx(x1, y1)] as f32;

    let i0 = p00 * (1.0 - dx) + p10 * dx;
    let i1 = p01 * (1.0 - dx) + p11 * dx;
    let v  = i0  * (1.0 - dy) + i1 * dy;

    v.round() as u8
}

#[inline] fn is_dark(v: u8) -> bool { v < 128 }

// ---------------- Гомография: unit square -> произвольный четырёхугольник ----------------

#[derive(Clone, Copy)]
struct Quad { p00: PointF, p10: PointF, p01: PointF, p11: PointF }

#[derive(Clone, Copy)]
struct ProjMap {
    x0: f32, x1: f32, x2: f32, x3: f32,
    y0: f32, y1: f32, y2: f32, y3: f32,
    g: f32, h: f32,
}

fn build_projective(quad: Quad) -> ProjMap {
    let (x0, y0) = (quad.p00.x, quad.p00.y);
    let (x1, y1) = (quad.p10.x - quad.p00.x, quad.p10.y - quad.p00.y);
    let (x2, y2) = (quad.p01.x - quad.p00.x, quad.p01.y - quad.p00.y);
    let (x3, y3) = (quad.p11.x - quad.p10.x - quad.p01.x + quad.p00.x,
                    quad.p11.y - quad.p10.y - quad.p01.y + quad.p00.y);

    let denom = x1 * y2 - y1 * x2;
    let (g, h) = if denom.abs() < 1e-6 { (0.0, 0.0) } else {
        let g = (x3 * y2 - y3 * x2) / denom;
        let h = (x1 * y3 - y1 * x3) / denom;
        (g, h)
    };
    ProjMap { x0, x1, x2, x3, y0, y1, y2, y3, g, h }
}

#[inline]
fn map_uv(pm: &ProjMap, u: f32, v: f32) -> PointF {
    let den = 1.0 + pm.g * u + pm.h * v;
    let x = (pm.x0 + pm.x1 * u + pm.x2 * v + pm.x3 * u * v) / den;
    let y = (pm.y0 + pm.y1 * u + pm.y2 * v + pm.y3 * u * v) / den;
    PointF { x, y }
}

// ------------------------- Осе-выровненный фоллбэк -------------------------

fn sample_axis_aligned_qr_v1(img: &GrayImage<'_>) -> Option<Vec<bool>> {
    if img.width % 29 != 0 || img.height % 29 != 0 { return None; }
    let unit_x = (img.width as f32) / 29.0;
    let unit_y = (img.height as f32) / 29.0;
    let qz = 4.0f32; // quiet zone
    let rx = unit_x * 0.35;
    let ry = unit_y * 0.35;

    eprintln!(
        "[sample/fallback] axis-aligned used: unit=({:.3},{:.3}) rx={:.2} ry={:.2}",
        unit_x, unit_y, rx, ry
    );

    let mut out = vec![false; N1 * N1];
    let mut preview = String::new();

    for y in 0..N1 {
        for x in 0..N1 {
            let cx = (qz + x as f32 + 0.5) * unit_x;
            let cy = (qz + y as f32 + 0.5) * unit_y;

            let x0 = (cx - rx).floor().max(0.0) as i32;
            let x1 = (cx + rx).floor().min((img.width - 1) as f32) as i32;
            let y0 = (cy - ry).floor().max(0.0) as i32;
            let y1 = (cy + ry).floor().min((img.height - 1) as f32) as i32;

            if x1 < x0 || y1 < y0 {
                out[y * N1 + x] = false;
                continue;
            }

            let mut sum: u32 = 0;
            let mut cnt: u32 = 0;
            for yy in y0..=y1 {
                let base = (yy as usize) * img.width;
                for xx in x0..=x1 {
                    sum += img.data[base + xx as usize] as u32;
                    cnt += 1;
                }
            }
            let avg = (sum / cnt.max(1)) as u8;
            let dark = avg < 128;
            out[y * N1 + x] = dark;

            if y < 8 && x < 8 {
                preview.push(if dark { '1' } else { '0' });
                if x == 7 { preview.push('\n'); }
            }
        }
    }

    eprintln!("[sample/fallback] preview 8x8:\n{}", preview);
    Some(out)
}

// ---------------------- «Почти осевой?» критерий ----------------------

fn is_near_axis_aligned(ux: PointF, uy: PointF) -> bool {
    let ux_len = (ux.x * ux.x + ux.y * uy.y).sqrt();
    let uy_len = (uy.x * uy.x + uy.y * uy.y).sqrt();
    if ux_len < 1e-3 || uy_len < 1e-3 { return false; }

    let dot = ux.x * uy.x + ux.y * uy.y;
    let cos_abs = (dot / (ux_len * uy_len)).abs();

    let shear_x = ux.y.abs() / (ux.x.abs() + 1e-6);
    let shear_y = uy.x.abs() / (uy.y.abs() + 1e-6);

    let angle_ok = cos_abs < 0.02;
    let shear_ok = shear_x < 0.05 && shear_y < 0.05;
    let scale_ok = ((ux_len - uy_len) / (ux_len + 1e-6)).abs() < 0.05;

    angle_ok && shear_ok && scale_ok
}

// ---------------------- Скоринг центральных таймингов ----------------------

fn timing_score_row_col<F>(get_bit: F) -> (f32, String, String)
where
    F: Fn(usize, usize) -> bool
{
    let y = 6usize; // timing row
    let x = 6usize; // timing col

    let mut row_bits: Vec<bool> = Vec::with_capacity(5);
    for xx in 8..=12 { row_bits.push(get_bit(xx, y)); }

    let mut col_bits: Vec<bool> = Vec::with_capacity(5);
    for yy in 8..=12 { col_bits.push(get_bit(x, yy)); }

    let mut alt_row = 0;
    for i in 0..row_bits.len().saturating_sub(1) {
        if row_bits[i] != row_bits[i+1] { alt_row += 1; }
    }
    let mut alt_col = 0;
    for i in 0..col_bits.len().saturating_sub(1) {
        if col_bits[i] != col_bits[i+1] { alt_col += 1; }
    }

    let denom_row = row_bits.len().saturating_sub(1).max(1) as f32;
    let denom_col = col_bits.len().saturating_sub(1).max(1) as f32;

    let score = (alt_row as f32 / denom_row + alt_col as f32 / denom_col) * 0.5;

    let row_str: String = row_bits.iter().map(|&b| if b {'1'} else {'0'}).collect();
    let col_str: String = col_bits.iter().map(|&b| if b {'1'} else {'0'}).collect();

    (score, row_str, col_str)
}

// ---------------------------- ОСНОВНОЙ СЭМПЛЕР ----------------------------

pub fn sample_qr_v1_grid(img: &GrayImage<'_>, _opts: &QrOptions, finders: &[PointF]) -> Option<Vec<bool>> {
    if finders.len() < 3 {
        eprintln!("[sample] ERROR: need 3 finders, got {}", finders.len());
        return None;
    }

    // Упорядочим как [BL, TL, TR]
    let [bl, tl, tr] = finder::order_finders([finders[0], finders[1], finders[2]]);

    // Векторы модуля (из центров фиднеров)
    let ux = PointF { x: (tr.x - tl.x) / 14.0, y: (tr.y - tl.y) / 14.0 };
    let uy = PointF { x: (bl.x - tl.x) / 14.0, y: (bl.y - tl.y) / 14.0 };
    let ux_len = (ux.x * ux.x + ux.y * ux.y).sqrt();
    let uy_len = (uy.x * uy.x + uy.y * uy.y).sqrt();

    // Внешние углы всего символа (0..20 по осям)
    let c00 = PointF { x: tl.x - 3.5*ux.x - 3.5*uy.x, y: tl.y - 3.5*ux.y - 3.5*uy.y }; // (0,0)
    let c10 = PointF { x: tl.x + 17.5*ux.x - 3.5*uy.x, y: tl.y + 17.5*ux.y - 3.5*uy.y }; // (20,0)
    let c01 = PointF { x: tl.x - 3.5*ux.x + 17.5*uy.x, y: tl.y - 3.5*ux.y + 17.5*uy.y }; // (0,20)
    let c11 = PointF { x: tl.x + 17.5*ux.x + 17.5*uy.x, y: tl.y + 17.5*ux.y + 17.5*uy.y }; // (20,20)

    let pm = build_projective(Quad { p00: c00, p10: c10, p01: c01, p11: c11 });

    eprintln!(
        "[sample] corners: C00=({:.2},{:.2}) C10=({:.2},{:.2}) C01=({:.2},{:.2}) C11=({:.2},{:.2}) |ux|={:.3}px |uy|={:.3}px",
        c00.x, c00.y, c10.x, c10.y, c01.x, c01.y, c11.x, c11.y, ux_len, uy_len
    );

    // Фоллбэк, если кадр реально осевой
    if (img.width % 29 == 0 && img.height % 29 == 0) && is_near_axis_aligned(ux, uy) {
        if let Some(bits) = sample_axis_aligned_qr_v1(img) { return Some(bits); }
    }

    // ======= Автокалибровка (анизотропные масштабы + сдвиги в норм. коорд) =======
    // u,v в [0..1], где u=(x+0.5)/21, v=(y+0.5)/21
    const SCALES: [f32; 5] = [0.985, 0.995, 1.000, 1.005, 1.015];
    const OFFS:   [f32; 5] = [-0.012, -0.006, 0.0, 0.006, 0.012]; // ~±0.25 модуля

    // суперсэмплинг: ±0.18 модуля в u,v → в норм. величинах:
    const SS: f32 = 0.18 / 21.0;
    const SS_OFFS: [f32; 3] = [-SS, 0.0, SS];

    let get_bit_with = |su: f32, sv: f32, du: f32, dv: f32, xx: usize, yy: usize| -> bool {
        let mut u0 = (xx as f32 + 0.5) / 21.0;
        let mut v0 = (yy as f32 + 0.5) / 21.0;
        u0 = (u0 * su + du).clamp(-0.02, 1.02);
        v0 = (v0 * sv + dv).clamp(-0.02, 1.02);

        let mut sum: u32 = 0;
        for dv_ in SS_OFFS {
            for du_ in SS_OFFS {
                let p = map_uv(&pm, u0 + du_, v0 + dv_);
                sum += sample_bilinear(img, p.x, p.y) as u32;
            }
        }
        let avg = (sum / 9) as u8;
        is_dark(avg)
    };

    let mut best = (f32::NEG_INFINITY, 1.0, 1.0, 0.0, 0.0, String::new(), String::new());
    for &su in &SCALES {
        for &sv in &SCALES {
            for &du in &OFFS {
                for &dv in &OFFS {
                    let (score, row_s, col_s) = timing_score_row_col(|x, y| get_bit_with(su, sv, du, dv, x, y));
                    if score > best.0 {
                        best = (score, su, sv, du, dv, row_s, col_s);
                    }
                }
            }
        }
    }

    let (score, su, sv, du, dv, row_s, col_s) = best;
    eprintln!(
        "[sample] tuning: su={:.3} sv={:.3} du={:.3} dv={:.3} timing_score={:.3}",
        su, sv, du, dv, score
    );
    eprintln!("[sample] row y=6 (x=8..12): {}", row_s);
    eprintln!("[sample] col x=6 (y=8..12): {}", col_s);

    // ======================= Окончательный сэмплинг =======================
    let mut out = vec![false; N1 * N1];
    let mut preview = String::new();

    for y in 0..N1 {
        for x in 0..N1 {
            let bit = get_bit_with(su, sv, du, dv, x, y);
            out[y * N1 + x] = bit;

            if y < 8 && x < 8 {
                preview.push(if bit { '1' } else { '0' });
                if x == 7 { preview.push('\n'); }
            }
        }
    }

    eprintln!("[sample] preview 8x8 (1=black,0=white):\n{}", preview);
    Some(out)
}
