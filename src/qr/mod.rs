//! QR-модуль: шаги 1–3 — поиск finder patterns, сэмплинг сетки v1 (21×21) и извлечение битов данных.
//!
//! Публичное API на этом этапе:
//! - `QrOptions` — опции сканирования;
//! - `PointF` — точка с float-координатами;
//! - `find_finder_patterns(&GrayImage, &QrOptions) -> Vec<PointF>` — до трёх центров;
//! - `synthesize_qr_v1_empty(unit)` — синтетика: только finders + quiet;
//! - `synthesize_qr_v1_skeleton(unit)` — finders + timing pattern + quiet;
//! - `sample_qr_v1_grid(&GrayImage, &QrOptions) -> Option<Vec<bool>>` — получить 21×21 битов (row-major);
//! - `extract_data_bits_v1(&[bool]) -> Vec<bool>` — вытащить 208 бит (включая data+EC) из матрицы v1.
//!
//! Следующие шаги (план):
//! - чтение format info (BCH(15,5)) → уровень коррекции + id маски;
//! - размаскировка → извлечение кодвордов → парсинг Byte mode;
//! - RS (GF(256)) для v1-L (один блок 19+7).

mod finder;
mod sample;
mod data;

pub use finder::{find_finder_patterns, synthesize_qr_v1_empty, synthesize_qr_v1_skeleton, PointF};
pub use sample::sample_qr_v1_grid;
pub use data::extract_data_bits_v1;

#[derive(Debug, Clone, Copy)]
pub struct QrOptions {
    /// Сколько строк/столбцов пробегать для поиска паттернов.
    pub scan_lines: usize,
}

impl Default for QrOptions {
    fn default() -> Self {
        Self { scan_lines: 24 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GrayImage;

    #[test]
    fn qr_finder_on_synthetic_v1() {
        // Сгенерим QR v1 (21×21 модулей) только с finder + quiet zone, unit=4 px/модуль
        let img = synthesize_qr_v1_empty(4);
        let opts = crate::qr::QrOptions { scan_lines: 32 };

        let pts = find_finder_patterns(&img, &opts);
        assert_eq!(pts.len(), 3, "ожидалось 3 центра finder");

        // Ожидаемые центры (в модульных координатах): TL/TR/BL на 3.5 от края области данных.
        let unit = 4.0f32;
        let qz = 4.0f32;   // quiet zone в модулях
        let n = 21.0f32;   // версия 1

        let tl = PointF { x: (qz + 3.5) * unit, y: (qz + 3.5) * unit };
        let tr = PointF { x: (qz + (n - 3.5)) * unit, y: (qz + 3.5) * unit };
        let bl = PointF { x: (qz + 3.5) * unit, y: (qz + (n - 3.5)) * unit };

        let r = 3.0 * unit;
        let r2 = r * r;

        let mut ok = 0usize;
        for p in &pts {
            if p.dist2(tl) <= r2 || p.dist2(tr) <= r2 || p.dist2(bl) <= r2 {
                ok += 1;
            }
        }
        assert_eq!(ok, 3, "найденные центры д.б. рядом с ожидаемыми");
    }

    #[test]
    fn qr_sampling_v1_timing_line() {
        // Скелет: finders + timing pattern (строка 6 и столбец 6), unit=4
        let img = synthesize_qr_v1_skeleton(4);
        let opts = QrOptions { scan_lines: 32 };

        let grid = sample_qr_v1_grid(&img, &opts).expect("grid must be sampled");
        assert_eq!(grid.len(), 21*21);

        // Проверяем, что тайминг-строка y=6 вне зон finder'ов чередуется 10101
        // По оси X это диапазон 8..=12 (колонка 13 — белый сепаратор TR).
        let y = 6usize;
        let expected = [true, false, true, false, true]; // начинаем с чёрного на x=8
        for (k, x) in (8usize..=12usize).enumerate() {
            let v = grid[y * 21 + x];
            assert_eq!(v, expected[k], "timing row x={x}");
        }

        // Аналогично тайминг-столбец x=6, диапазон y=8..=12
        let x = 6usize;
        let expected2 = [true, false, true, false, true];
        for (k, y) in (8usize..=12usize).enumerate() {
            let v = grid[y * 21 + x];
            assert_eq!(v, expected2[k], "timing col y={y}");
        }
    }

    #[test]
    fn qr_extract_data_bits_count() {
        // Берём skeleton (изображение), сэмплируем в матрицу и извлекаем биты.
        let img = synthesize_qr_v1_skeleton(4);
        let opts = QrOptions { scan_lines: 32 };
        let grid = sample_qr_v1_grid(&img, &opts).expect("grid");
        let bits = extract_data_bits_v1(&grid);
        // Для v1 всего 26 кодвордов → 26×8 = 208 модулей данных (включая EC).
        assert_eq!(bits.len(), 208, "v1 must have 208 data+ec bits");
    }

    #[test]
    fn qr_extract_data_bits_order_sanity() {
        // Синтетическая матрица 21×21: пометим первые K ячеек обхода как true.
        // Потом экстракт должен вернуть K первых бит = true, остальные false.
        let mut grid = vec![false; 21*21];

        // Помечаем служебные области (они уже будут пропускаться экстрактором) — не обязательно.
        // Просто отметим первые K данных.
        let k = 40usize;
        mark_first_k_data_modules(&mut grid, k);

        let bits = extract_data_bits_v1(&grid);
        assert_eq!(bits.len(), 208);

        // Первые k — true, дальше false.
        let mut true_cnt = 0usize;
        for (i, b) in bits.iter().enumerate() {
            if i < k { assert_eq!(*b, true, "bit {i}"); true_cnt += 1; }
            else { assert_eq!(*b, false, "bit {i}"); }
        }
        assert_eq!(true_cnt, k);
    }

    // Вспомогалка для теста: в порядке сканирования помечает первые k модулей данных.
    fn mark_first_k_data_modules(grid: &mut [bool], k: usize) {
        use data::{is_function_v1, walk_pairs_v1};
        let mut left = k;
        for (x,y) in walk_pairs_v1() {
            if is_function_v1(x,y) { continue; }
            let idx = y*21 + x;
            grid[idx] = true;
            left -= 1;
            if left == 0 { break; }
        }
    }

    // Sanity на API
    #[test]
    fn api_smoke() {
        let img = GrayImage { width: 21, height: 21, data: &vec![255u8; 21*21] };
        let _ = find_finder_patterns(&img, &QrOptions::default());
        let _ = sample_qr_v1_grid(&img, &QrOptions::default());
        let _ = extract_data_bits_v1(&vec![false; 21*21]);
    }
}
