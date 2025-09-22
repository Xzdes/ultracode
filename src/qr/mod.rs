//! QR-модуль: v1 (21×21), шаги 1–5 — finder, сэмплинг, извлечение данных,
//! формат (EC + mask), размаскировка, bytes/bit→Byte mode, энд-ту-энд декод.

mod finder;
mod sample;
mod data;
mod format;
mod bytes;
mod rs;
mod encode;

pub use finder::{find_finder_patterns, synthesize_qr_v1_empty, synthesize_qr_v1_skeleton, PointF};
pub use sample::sample_qr_v1_grid;
pub use data::extract_data_bits_v1;
pub use format::{decode_format_info_v1, unmask_grid_v1, EcLevel};
pub use bytes::{bits_to_bytes_v1, parse_byte_mode_v1_l, parse_byte_mode_bits_v1_l, parse_byte_mode_bits_v1_l_relaxed};
pub use encode::synthesize_qr_v1_from_text;

use crate::GrayImage;
use rs::rs_ec_bytes;

#[derive(Debug, Clone, Copy)]
pub struct QrOptions {
    pub scan_lines: usize,
}

impl Default for QrOptions {
    fn default() -> Self { Self { scan_lines: 24 } }
}

/// Полный декод v1-L (Byte mode) из изображения.
/// Пытаемся по формату; если не вышло — перебираем маски.
/// Парсим **напрямую из битов** (без упаковки в байты), а затем — по ECC-совпадению.
pub fn decode_qr_v1_l_text(img: &GrayImage<'_>, opts: &QrOptions) -> Option<String> {
    let grid0 = sample_qr_v1_grid(img, opts)?;

    // Сначала пробуем по форматной информации (идеальный путь).
    if let Some((_, mask_id)) = decode_format_info_v1(&grid0) {
        if let Some(text) = try_decode_with_mask_bits(&grid0, mask_id) {
            return Some(text);
        }
    }

    // Фолбэк: перебираем все 8 масок (на случай, если формат прочитался плохо).
    for mask in 0u8..=7 {
        if let Some(text) = try_decode_with_mask_bits(&grid0, mask) {
            return Some(text);
        }
    }

    None
}

fn try_decode_with_mask_bits(grid_src: &[bool], mask_id: u8) -> Option<String> {
    // Размаскируем копию (только data-модули)
    let mut grid = grid_src.to_vec();
    unmask_grid_v1(&mut grid, mask_id);

    // Вытащим биты данных (208 бит)
    let bits = extract_data_bits_v1(&grid);

    // Кандидатные трансформации потока бит
    let try_variants: &[fn(&[bool]) -> Vec<bool>] = &[
        |b| b.to_vec(),                                   // 1) как есть
        invert_all,                                       // 2) инверсия
        |b| reverse_bits_in_each_byte(b, 19*8),           // 3) реверс бит внутри каждого байта
        |b| invert_all(&reverse_bits_in_each_byte(b, 19*8)), // 4) (3)+инверсия
        reverse_all,                                      // 5) разворот всего потока
        |b| invert_all(&reverse_all(b)),                  // 6) (5)+инверсия
        |b| reverse_byte_order(b, 19*8),                  // 7) разворот порядка байтов (data-часть)
        |b| invert_all(&reverse_byte_order(b, 19*8)),     // 8) (7)+инверсия
        |b| reverse_bits_in_each_byte(&reverse_byte_order(b, 19*8), 19*8), // 9) (7)+реверс бит
        |b| invert_all(&reverse_bits_in_each_byte(&reverse_byte_order(b, 19*8), 19*8)), // 10) + инверсия
    ];

    // 1) Быстрые попытки: строгий парсер, затем relaxed. Пустую строку игнорируем.
    for tf in try_variants {
        let stream = tf(&bits);
        if let Some(txt) = parse_byte_mode_bits_v1_l(&stream) {
            if !txt.is_empty() { return Some(txt); }
        }
        if let Some(txt) = parse_byte_mode_bits_v1_l_relaxed(&stream) {
            if !txt.is_empty() { return Some(txt); }
        }
    }

    // 2) Надёжный фолбэк: валидация по RS-ECC (совпадение 7 байт паритета).
    // Для каждой трансформации упакуем кодворды и проверим, совпадают ли рассчитанные ECC.
    for tf in try_variants {
        let stream = tf(&bits);
        let cw = bits_to_bytes_v1(&stream);
        if cw.len() < 26 { continue; }
        let (data_cw, ecc_cw) = (&cw[..19], &cw[19..26]);
        let ecc_calc = rs_ec_bytes(data_cw, 7);
        if ecc_calc.as_slice() == ecc_cw {
            if let Some(txt) = parse_byte_mode_v1_l(data_cw) {
                if !txt.is_empty() { return Some(txt); }
            }
        }
    }

    None
}

/// Инверсия всех бит.
fn invert_all(bits: &[bool]) -> Vec<bool> {
    bits.iter().map(|&b| !b).collect()
}

/// Разворот всего потока.
fn reverse_all(bits: &[bool]) -> Vec<bool> {
    let mut v = bits.to_vec();
    v.reverse();
    v
}

/// Переставляет биты внутри каждого 8-битного блока (MSB↔LSB), только для первых `n_bits` бит.
fn reverse_bits_in_each_byte(bits: &[bool], n_bits: usize) -> Vec<bool> {
    let n = bits.len().min(n_bits);
    let mut out = bits.to_vec();
    let mut i = 0usize;
    while i + 8 <= n {
        for k in 0..4 { out.swap(i + k, i + 7 - k); }
        i += 8;
    }
    out.truncate(n);
    out
}

/// Разворот порядка байтов в первых `n_bits` (кратно 8) битах.
fn reverse_byte_order(bits: &[bool], n_bits: usize) -> Vec<bool> {
    let n = bits.len().min(n_bits);
    let n_bytes = n / 8;
    let mut out = bits.to_vec();
    for i in 0..n_bytes {
        let src = &bits[i*8 .. i*8 + 8];
        let dst_pos = (n_bytes - 1 - i) * 8;
        out[dst_pos .. dst_pos + 8].copy_from_slice(src);
    }
    out.truncate(n);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GrayImage;

    #[test]
    fn qr_finder_on_synthetic_v1() {
        let img = synthesize_qr_v1_empty(4);
        let opts = QrOptions { scan_lines: 32 };
        let pts = find_finder_patterns(&img, &opts);
        assert_eq!(pts.len(), 3);

        let unit = 4.0f32;
        let qz = 4.0f32;
        let n = 21.0f32;

        let tl = PointF { x: (qz + 3.5) * unit, y: (qz + 3.5) * unit };
        let tr = PointF { x: (qz + (n - 3.5)) * unit, y: (qz + 3.5) * unit };
        let bl = PointF { x: (qz + 3.5) * unit, y: (qz + (n - 3.5)) * unit };
        let r2 = (3.0 * unit).powi(2);
        let mut ok = 0usize;
        for p in &pts {
            if p.dist2(tl) <= r2 || p.dist2(tr) <= r2 || p.dist2(bl) <= r2 { ok += 1; }
        }
        assert_eq!(ok, 3);
    }

    #[test]
    fn qr_sampling_v1_timing_line() {
        let img = synthesize_qr_v1_skeleton(4);
        let opts = QrOptions { scan_lines: 32 };
        let grid = sample_qr_v1_grid(&img, &opts).expect("grid");
        assert_eq!(grid.len(), 21*21);

        // y=6, x=8..=12  → 10101
        let y = 6usize;
        let expected = [true, false, true, false, true];
        for (k, x) in (8usize..=12usize).enumerate() {
            assert_eq!(grid[y * 21 + x], expected[k]);
        }
        // x=6, y=8..=12  → 10101
        let x = 6usize;
        let expected2 = [true, false, true, false, true];
        for (k, y) in (8usize..=12usize).enumerate() {
            assert_eq!(grid[y * 21 + x], expected2[k]);
        }
    }

    #[test]
    fn qr_extract_data_bits_count() {
        let img = synthesize_qr_v1_skeleton(4);
        let opts = QrOptions { scan_lines: 32 };
        let grid = sample_qr_v1_grid(&img, &opts).expect("grid");
        let bits = extract_data_bits_v1(&grid);
        assert_eq!(bits.len(), 208);
    }

    #[test]
    fn qr_extract_data_bits_order_sanity() {
        use data::{is_function_v1, walk_pairs_v1};
        let mut grid = vec![false; 21*21];
        let mut left = 40usize;
        for (x,y) in walk_pairs_v1() {
            if is_function_v1(x,y) { continue; }
            grid[y*21 + x] = true;
            left -= 1; if left==0 { break; }
        }
        let bits = extract_data_bits_v1(&grid);
        let ones = bits.iter().take(40).filter(|b| **b).count();
        assert_eq!(ones, 40);
        assert!(!bits.iter().skip(40).any(|&b| b));
    }

    #[test]
    fn qr_end_to_end_v1_l_hello() {
        let img = synthesize_qr_v1_from_text("HELLO 123", 3, 4);
        let opts = QrOptions { scan_lines: 32 };
        let text = decode_qr_v1_l_text(&img, &opts).expect("decode");
        assert_eq!(text, "HELLO 123");
    }

    // Sanity API
    #[test]
    fn api_smoke() {
        let img = GrayImage { width: 21, height: 21, data: &vec![255u8; 21*21] };
        let _ = find_finder_patterns(&img, &QrOptions::default());
        let _ = sample_qr_v1_grid(&img, &QrOptions::default());
        let _ = extract_data_bits_v1(&vec![false; 21*21]);
        let _ = decode_format_info_v1(&vec![false; 21*21]);
        let _ = bits_to_bytes_v1(&vec![false; 208]);
    }
}
