//! QR-модуль: v1 (21×21), шаги 1–5 — finder, сэмплинг, извлечение данных,
//! формат (EC + mask), размаскировка, bytes/bit→Byte mode, энд-ту-энд декод.
//! С расширенным логированием + полный перебор 8 симметрий (4 поворота × зеркало).

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

// ======== утилиты логирования ========

#[inline]
fn bits_prefix(bits: &[bool], n: usize) -> String {
    let mut s = String::with_capacity(n.min(bits.len()));
    for &b in bits.iter().take(n) {
        s.push(if b { '1' } else { '0' });
    }
    s
}

#[inline]
fn count_equal_bytes(a: &[u8], b: &[u8]) -> u8 {
    a.iter().zip(b.iter()).map(|(x, y)| u8::from(x == y)).sum()
}

#[derive(Debug, Default)]
struct DebugAcc {
    best_bytes: u8,
    best_desc: String,
}

#[derive(Debug, Clone, Copy)]
pub struct QrOptions {
    pub scan_lines: usize,
}

impl Default for QrOptions {
    fn default() -> Self { Self { scan_lines: 24 } }
}

/// Полный декод v1-L (Byte mode) из изображения.
/// Перебираем 2 варианта зеркала (нет/горизонт), внутри — 4 ориентации, затем:
/// маска из форматных бит (если прочлась), потом брутфорс всех 8 масок.
/// В каждой ветке — обе упаковки (MSB/LSB), сдвиги 0..7, обе сегментации (19+7 / 7+19)
/// и куча безопасных трансформаций битового потока. Результат принимаем ТОЛЬКО после
/// успешной RS-валидации (7 байт).
pub fn decode_qr_v1_l_text(img: &GrayImage<'_>, opts: &QrOptions) -> Option<String> {
    let grid0 = sample_qr_v1_grid(img, opts)?;
    // безопасная печать количества пикселей
    let px: u128 = (img.width as u128) * (img.height as u128);
    eprintln!("[QR] sample grid ok ({} px) → 21×21", px);

    // Переберём «диэдральную восьмёрку»: зеркало ∈ {none, horiz} × поворот ∈ {0,1,2,3}
    for mirror in [false, true] {
        let grid_m = if mirror { reflect21_h(&grid0) } else { grid0.clone() };
        eprintln!("[QR] mirror = {}", if mirror { "H" } else { "none" });

        for rot in 0..4u8 {
            let grid_rot = rotate21(&grid_m, rot);
            let deg: u16 = (rot as u16) * 90; // <-- фикс переполнения u8
            eprintln!("[QR] rotate = {}°", deg);

            // Быстрый путь: если формат прочитался, пробуем соответствующую маску
            if let Some((ec, mask_id)) = decode_format_info_v1(&grid_rot) {
                eprintln!("[QR]  format ok: ec={:?}, mask={}", ec, mask_id);
                if let Some(text) = try_decode_with_mask_bits(&grid_rot, mask_id) {
                    eprintln!("[QR]  SUCCESS by format/mask={}", mask_id);
                    return Some(text);
                } else {
                    eprintln!("[QR]  format/mask={} → no decode, will try all masks", mask_id);
                }
            } else {
                eprintln!("[QR]  format read: NONE");
            }

            // Фолбэк: перебираем все 8 масок
            for mask in 0u8..=7 {
                if let Some(text) = try_decode_with_mask_bits(&grid_rot, mask) {
                    eprintln!("[QR]  SUCCESS by brute mask={}", mask);
                    return Some(text);
                }
            }
        }
    }

    eprintln!("[QR] FAIL: no valid variant matched RS-ECC");
    None
}

/// Попытка декодирования для фиксированной маски (на одной ориентации сетки).
/// ВАЖНО: принимаем результат ТОЛЬКО после совпадения RS-ECC.
fn try_decode_with_mask_bits(grid_src: &[bool], mask_id: u8) -> Option<String> {
    eprintln!("[QR]  try mask={}", mask_id);

    // Ветка A: классическая — размаскируем копию (только data-модули)
    let mut grid_unmasked = grid_src.to_vec();
    unmask_grid_v1(&mut grid_unmasked, mask_id);

    // Ветка B: без размаскировки (на случай расхождения конвенции)
    let grid_masked = grid_src; // используем как есть

    // Извлечём биты данных (208 бит) в обоих вариантах
    let bits_unmasked = extract_data_bits_v1(&grid_unmasked);
    let bits_masked   = extract_data_bits_v1(grid_masked);

    eprintln!(
        "[QR]   branch=unmask bits[0..32]={}",
        bits_prefix(&bits_unmasked, 32)
    );
    eprintln!(
        "[QR]   branch=masked bits[0..32]={}",
        bits_prefix(&bits_masked, 32)
    );

    // Прогоним обе базовые ветки через единый «комбайнер» вариантов
    let mut acc = DebugAcc::default();
    if let Some(txt) = try_all_stream_variants("unmask", &bits_unmasked, &mut acc) {
        return Some(txt);
    }
    if let Some(txt) = try_all_stream_variants("masked", &bits_masked, &mut acc) {
        return Some(txt);
    }

    eprintln!(
        "[QR]   mask={} → no RS match; best parity match: {}/7 @ {}",
        mask_id, acc.best_bytes, acc.best_desc
    );
    None
}

/// Проверить поток: порождение трансформаций, сдвиги, упаковки, обе сегментации, RS-валидация и парсинг.
/// ДОБАВЛЕНО: перебор битовых сдвигов 0..7 для выравнивания начала потока.
/// ДОБАВЛЕНО: вариации на уровне всех 26 кодвордов (rev/revbits).
fn try_all_stream_variants(branch: &str, bits: &[bool], acc: &mut DebugAcc) -> Option<String> {
    // Кандидатные трансформации потока бит (устойчивость к полярности/порядку/реверсам)
    let variants: [(&str, Box<dyn Fn(&[bool]) -> Vec<bool>>); 16] = [
        ("id", Box::new(|b| b.to_vec())),
        ("invert", Box::new(|b| invert_all(b))),
        ("rev", Box::new(|b| reverse_all(b))),
        ("rev+invert", Box::new(|b| invert_all(&reverse_all(b)))),
        // для первых 19 байт (данные)
        ("revBytes19", Box::new(|b| reverse_byte_order_bits(b, 19*8))),
        ("revBytes19+inv", Box::new(|b| invert_all(&reverse_byte_order_bits(b, 19*8)))),
        ("revBitsEach19", Box::new(|b| reverse_bits_in_each_byte(b, 19*8))),
        ("revBitsEach19+inv", Box::new(|b| invert_all(&reverse_bits_in_each_byte(b, 19*8)))),
        ("revBytes19+revBits", Box::new(|b| reverse_bits_in_each_byte(&reverse_byte_order_bits(b, 19*8), 19*8))),
        ("revBytes19+revBits+inv", Box::new(|b| invert_all(&reverse_bits_in_each_byte(&reverse_byte_order_bits(b, 19*8), 19*8)))),
        // для всех 26 байт (data+ecc)
        ("revBytes26", Box::new(|b| reverse_byte_order_bits(b, 26*8))),
        ("revBytes26+inv", Box::new(|b| invert_all(&reverse_byte_order_bits(b, 26*8)))),
        ("revBitsEach26", Box::new(|b| reverse_bits_in_each_byte(b, 26*8))),
        ("revBitsEach26+inv", Box::new(|b| invert_all(&reverse_bits_in_each_byte(b, 26*8)))),
        ("revBytes26+revBits", Box::new(|b| reverse_bits_in_each_byte(&reverse_byte_order_bits(b, 26*8), 26*8))),
        ("revBytes26+revBits+inv", Box::new(|b| invert_all(&reverse_bits_in_each_byte(&reverse_byte_order_bits(b, 26*8), 26*8)))),
    ];

    for (xname, xform) in variants.iter() {
        let stream = xform(bits);
        // Перебор сдвигов 0..7: берём ровно 208 бит из окна [shift .. shift+208)
        for shift in 0..8usize {
            if stream.len() < shift + 208 { break; }
            let slice = &stream[shift .. shift + 208];

            if let Some(txt) = try_stream_with_all_packings(branch, xname, shift, slice, acc) {
                eprintln!("[QR]   OK: branch={}, xform={}, shift={}", branch, xname, shift);
                return Some(txt);
            }
        }
    }
    None
}

/// Проверить поток: MSB-first и LSB-first упаковки + обе сегментации кодвордов,
/// RS-валидация и парсинг Byte mode.
fn try_stream_with_all_packings(
    branch: &str,
    xname: &str,
    shift: usize,
    stream208: &[bool],
    acc: &mut DebugAcc,
) -> Option<String> {
    debug_assert_eq!(stream208.len(), 208);

    // 1) MSB-first упаковка
    if let Some(txt) = try_stream_with_pack_and_segments(branch, xname, shift, "MSB", stream208, false, acc) {
        return Some(txt);
    }
    // 2) LSB-first: эквивалент MSB-упаковке после реверса бит в каждом байте
    let lsb_like = reverse_bits_in_each_byte(stream208, stream208.len());
    try_stream_with_pack_and_segments(branch, xname, shift, "LSB", &lsb_like, true, acc)
}

/// Вспом: проверить обе сегментации (data+ecc и ecc+data), с RS-валидацией.
/// Параметр `lsb_pack` влияет только на то, как интерпретировать байты данных при парсинге.
fn try_stream_with_pack_and_segments(
    branch: &str,
    xname: &str,
    shift: usize,
    packing: &str,
    stream_for_pack: &[bool],
    lsb_pack: bool,
    acc: &mut DebugAcc,
) -> Option<String> {
    let cw = bits_to_bytes_v1(stream_for_pack);
    if cw.len() < 26 { return None; }

    // Все варианты сегментации 26 кодвордов
    let segments: [(&str, &[u8], &[u8]); 2] = [
        ("data+ecc", &cw[0..19], &cw[19..26]), // нормальная: 19 data + 7 ecc
        ("ecc+data", &cw[7..26], &cw[0..7]),   // альтернативная: 7 ecc + 19 data
    ];

    for (segname, data_cw, ecc_cw) in segments {
        // RS-ECC на 19 байтах данных
        let ecc_calc = rs_ec_bytes(data_cw, 7);

        if ecc_calc.as_slice() == ecc_cw {
            // ECC совпали — парсим Byte mode.
            let data_norm: Vec<u8> = if lsb_pack {
                data_cw.iter().map(|b| bitrev8(*b)).collect()
            } else {
                data_cw.to_vec()
            };

            if let Some(txt) = parse_byte_mode_v1_l(&data_norm) {
                if !txt.is_empty() {
                    return Some(txt);
                }
            }

            if let Some(txt) = parse_byte_mode_bits_v1_l(stream_for_pack)
                .or_else(|| parse_byte_mode_bits_v1_l_relaxed(stream_for_pack))
            {
                if !txt.is_empty() {
                    return Some(txt);
                }
            }
        } else {
            // Обновим лучший «почти матч»: сколько байт ECC совпало.
            let eq = count_equal_bytes(&ecc_calc, ecc_cw);
            if eq > acc.best_bytes {
                acc.best_bytes = eq;
                acc.best_desc = format!(
                    "branch={}, xform={}, shift={}, packing={}, seg={}",
                    branch, xname, shift, packing, segname
                );
            }
        }
    }

    None
}

// === Инструменты битовых трансформаций ===

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

/// Разворот порядка байтов только в первых `n_bits` (кратно 8) битах потока.
fn reverse_byte_order_bits(bits: &[bool], n_bits: usize) -> Vec<bool> {
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

/// Поворот квадратной сетки 21×21 на 0/90/180/270 градусов по часовой стрелке.
fn rotate21(grid: &[bool], rot: u8) -> Vec<bool> {
    const N: usize = 21;
    debug_assert_eq!(grid.len(), N*N);
    let mut out = vec![false; N*N];

    match rot % 4 {
        0 => {
            out.copy_from_slice(grid);
        }
        1 => { // 90°: (x,y) -> (N-1-y, x)
            for y in 0..N {
                for x in 0..N {
                    let nx = N - 1 - y;
                    let ny = x;
                    out[ny*N + nx] = grid[y*N + x];
                }
            }
        }
        2 => { // 180°: (x,y) -> (N-1-x, N-1-y)
            for y in 0..N {
                for x in 0..N {
                    let nx = N - 1 - x;
                    let ny = N - 1 - y;
                    out[ny*N + nx] = grid[y*N + x];
                }
            }
        }
        3 => { // 270°: (x,y) -> (y, N-1-x)
            for y in 0..N {
                for x in 0..N {
                    let nx = y;
                    let ny = N - 1 - x;
                    out[ny*N + nx] = grid[y*N + x];
                }
            }
        }
        _ => unreachable!(),
    }
    out
}

/// Горизонтальное зеркало квадратной сетки 21×21.
fn reflect21_h(grid: &[bool]) -> Vec<bool> {
    const N: usize = 21;
    debug_assert_eq!(grid.len(), N*N);
    let mut out = vec![false; N*N];
    for y in 0..N {
        for x in 0..N {
            out[y*N + (N - 1 - x)] = grid[y*N + x];
        }
    }
    out
}

/// Реверс бит в байте.
#[inline]
fn bitrev8(x: u8) -> u8 {
    let mut v = x;
    v = (v >> 4) | (v << 4);
    v = ((v & 0x33) << 2) | ((v & 0xCC) >> 2);
    v = ((v & 0x55) << 1) | ((v & 0xAA) >> 1);
    v
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
