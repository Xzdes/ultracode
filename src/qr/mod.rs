//! QR-модуль: шаги 1–4 — finder patterns, сэмплинг сетки v1 (21×21), извлечение битов данных,
//! чтение format info (EC + mask) и размаскировка.
//!
//! Публичное API:
//! - `QrOptions`
//! - `PointF`
//! - `find_finder_patterns(&GrayImage, &QrOptions) -> Vec<PointF>`
//! - `synthesize_qr_v1_empty(unit)`
//! - `synthesize_qr_v1_skeleton(unit)`
//! - `sample_qr_v1_grid(&GrayImage, &QrOptions) -> Option<Vec<bool>>`
//! - `extract_data_bits_v1(&[bool]) -> Vec<bool>`
//! - `decode_format_info_v1(&[bool]) -> Option<(EcLevel, u8)>`
//! - `unmask_grid_v1(&mut [bool], u8)`

mod finder;
mod sample;
mod data;
mod format;

pub use finder::{find_finder_patterns, synthesize_qr_v1_empty, synthesize_qr_v1_skeleton, PointF};
pub use sample::sample_qr_v1_grid;
pub use data::extract_data_bits_v1;
pub use format::{decode_format_info_v1, unmask_grid_v1, EcLevel};

#[derive(Debug, Clone, Copy)]
pub struct QrOptions {
    pub scan_lines: usize,
}

impl Default for QrOptions {
    fn default() -> Self { Self { scan_lines: 24 } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GrayImage;
    use super::format::encode_format_bits_for_tests as enc_fmt;

    #[test]
    fn qr_finder_on_synthetic_v1() {
        let img = synthesize_qr_v1_empty(4);
        let opts = crate::qr::QrOptions { scan_lines: 32 };
        let pts = find_finder_patterns(&img, &opts);
        assert_eq!(pts.len(), 3, "ожидалось 3 центра finder");

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
        let grid = sample_qr_v1_grid(&img, &opts).expect("grid must be sampled");
        assert_eq!(grid.len(), 21*21);

        let y = 6usize;
        let expected = [true, false, true, false, true];
        for (k, x) in (8usize..=12usize).enumerate() {
            let v = grid[y * 21 + x];
            assert_eq!(v, expected[k], "timing row x={x}");
        }
        let x = 6usize;
        let expected2 = [true, false, true, false, true];
        for (k, y) in (8usize..=12usize).enumerate() {
            let v = grid[y * 21 + x];
            assert_eq!(v, expected2[k], "timing col y={y}");
        }
    }

    #[test]
    fn qr_extract_data_bits_count() {
        let img = synthesize_qr_v1_skeleton(4);
        let opts = QrOptions { scan_lines: 32 };
        let grid = sample_qr_v1_grid(&img, &opts).expect("grid");
        let bits = extract_data_bits_v1(&grid);
        assert_eq!(bits.len(), 208, "v1 must have 208 data+ec bits");
    }

    #[test]
    fn qr_extract_data_bits_order_sanity() {
        let mut grid = vec![false; 21*21];
        let k = 40usize;
        mark_first_k_data_modules(&mut grid, k);
        let bits = extract_data_bits_v1(&grid);
        assert_eq!(bits.len(), 208);
        for (i, b) in bits.iter().enumerate() {
            if i < k { assert_eq!(*b, true, "bit {i}"); } else { assert!(!*b, "bit {i}"); }
        }
    }

    #[test]
    fn qr_format_decode_and_unmask_roundtrip() {
        // 1) получаем skeleton-изображение → сэмплируем в матрицу
        let img = synthesize_qr_v1_skeleton(4);
        let opts = QrOptions { scan_lines: 32 };
        let mut grid = sample_qr_v1_grid(&img, &opts).expect("grid");

        // 2) впишем обе копии format info с EC=L и mask=3
        write_format_into_grid(&mut grid, EcLevel::L, 3);

        // 3) декодируем формат
        let (ec, mask) = decode_format_info_v1(&grid).expect("format");
        assert_eq!(ec, EcLevel::L);
        assert_eq!(mask, 3);

        // 4) пометим первые 50 data-модулей как true → «данные»
        mark_first_k_data_modules(&mut grid, 50);
        let orig_bits = super::data::extract_data_bits_v1(&grid);

        // 5) применим маску (та же функция обратима) и снимем её обратно
        super::unmask_grid_v1(&mut grid, mask); // «замаскировали» данные
        super::unmask_grid_v1(&mut grid, mask); // сняли маску
        let roundtrip_bits = super::data::extract_data_bits_v1(&grid);
        assert_eq!(orig_bits, roundtrip_bits, "маска должна быть обратима");
    }

    // === Вспомогалки для тестов ===

    fn write_format_into_grid(grid: &mut [bool], ec: EcLevel, mask_id: u8) {
        let code = enc_fmt(ec, mask_id); // уже с BCH и XOR-маской
        // Записываем в обе копии по нашему же порядку чтения.

        // Copy A: y=8, x=0..=8 (кроме 6); x=8, y=8..=0 (кроме 8 и 6)
        let mut bits = code;
        let mut take = |grid: &mut [bool], x: usize, y: usize, bits: &mut u16| {
            let b = ((*bits >> 14) & 1) != 0;
            *bits <<= 1;
            grid[y*21 + x] = b;
        };
        for x in 0..=8 {
            if x == 6 { continue; }
            take(grid, x, 8, &mut bits);
        }
        for y in (0..=8).rev() {
            if y == 8 || y == 6 { continue; }
            take(grid, 8, y, &mut bits);
        }

        // Copy B: y=8, x=20..=13; x=8, y=20..=14
        let mut bits2 = code;
        for x in (13..=20).rev() {
            take(grid, x, 8, &mut bits2);
        }
        for y in (14..=20).rev() {
            take(grid, 8, y, &mut bits2);
        }
    }

    fn mark_first_k_data_modules(grid: &mut [bool], k: usize) {
        use super::data::{is_function_v1, walk_pairs_v1};
        let mut left = k;
        for (x,y) in walk_pairs_v1() {
            if is_function_v1(x,y) { continue; }
            grid[y*21 + x] = true;
            left -= 1;
            if left == 0 { break; }
        }
    }

    // Sanity
    #[test]
    fn api_smoke() {
        let img = GrayImage { width: 21, height: 21, data: &vec![255u8; 21*21] };
        let _ = find_finder_patterns(&img, &QrOptions::default());
        let _ = sample_qr_v1_grid(&img, &QrOptions::default());
        let _ = extract_data_bits_v1(&vec![false; 21*21]);
        let _ = decode_format_info_v1(&vec![false; 21*21]);
    }
}
