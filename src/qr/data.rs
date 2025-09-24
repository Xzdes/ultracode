//! QR v1 (21×21): служебные зоны и порядок обхода «парами колонок».
//!
//! Здесь три ключевые вещи:
//! 1) [`is_function_v1`] — пометка служебных модулей (finder+separator, timing,
//!    format и т.п.) — они не несут data/ECC бит.
//! 2) [`walk_pairs_v1`] — правильный маршрут чтения модулей для извлечения бит:
//!    идём парами колонок (x, x-1) справа налево, «змейкой» по y. Колонку x=6
//!    (timing) пропускаем как пару — т.е. после x=8,7 сразу x=5,4.
//! 3) [`extract_data_bits_v1`] — снимаем только data-модули (ровно 208 бит для v1).

/// Размер сетки для версии 1.
pub const N1: usize = 21;

/// Является ли модуль служебным (не data/ECC) для QR v1.
///
/// Покрываем:
/// - Finder + белые сепараторы вокруг (три угла): прямоугольники 9×9 / 8×9 / 9×8.
/// - Timing-линии: вся колонка x=6 и вся строка y=6 (кроме зон finder — они уже
///   попадают в прямоугольники).
/// - Формат-поля попадают в эти зоны.
/// - «Тёмный модуль» v1 оказывается в левом нижнем прямоугольнике, отдельно
///   его помечать не нужно.
#[inline]
pub fn is_function_v1(x: usize, y: usize) -> bool {
    debug_assert!(x < N1 && y < N1);

    // Finder+separator прямоугольники:
    if (x <= 8 && y <= 8)            // левый верхний 9×9
        || (x >= N1 - 8 && y <= 8)   // правый верхний 8×9
        || (x <= 8 && y >= N1 - 8)
    // левый нижний 9×8
    {
        return true;
    }

    // Timing-линии:
    if x == 6 || y == 6 {
        return true;
    }

    false
}

/// Маршрут обхода для выборки бит: пары колонок (x, x-1), справа налево,
/// «змейкой» по y. Пару с x=6 (timing-колонка) пропускаем целиком: после
/// пары (8,7) сразу идём на (5,4), затем (3,2), (1,0).
///
/// Возвращает порядок координат модулей для **всей сетки, кроме x=6**.
/// Колонка x=6 отсутствует намеренно (420 координат).
pub fn walk_pairs_v1() -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(N1 * N1);

    let mut x: isize = (N1 as isize) - 1; // 20 при N1=21
    let mut upward = true; // первая пара идём вверх

    // пары колонок: (x, x-1)
    while x > 0 {
        let xx = x as usize;
        if upward {
            for y in (0..N1).rev() {
                out.push((xx, y));
                out.push((xx - 1, y));
            }
        } else {
            for y in 0..N1 {
                out.push((xx, y));
                out.push((xx - 1, y));
            }
        }
        upward = !upward; // направление меняем после КАЖДОЙ пары
        x -= 2;
    }

    // ДОБАВЛЯЕМ последнюю одинарную колонку x==0
    if x == 0 {
        if upward {
            for y in (0..N1).rev() {
                out.push((0, y));
            }
        } else {
            for y in 0..N1 {
                out.push((0, y));
            }
        }
    }

    debug_assert_eq!(out.len(), N1 * N1); // теперь 441
    out
}

/// Снять ровно 208 data-бит (без служебных) согласно маршруту [`walk_pairs_v1`].
pub fn extract_data_bits_v1(grid: &[bool]) -> Vec<bool> {
    debug_assert_eq!(grid.len(), N1 * N1);

    let mut bits = Vec::with_capacity(208);
    for (x, y) in walk_pairs_v1() {
        if is_function_v1(x, y) {
            continue;
        }
        bits.push(grid[y * N1 + x]);
        if bits.len() == 208 {
            break;
        }
    }
    bits
}

/// Предикаты восьми масок из ISO/IEC 18004 (0..7).
#[inline]
pub(crate) fn mask_predicate(mask_id: u8, x: usize, y: usize) -> bool {
    let x = x as i32;
    let y = y as i32;
    match mask_id & 7 {
        0 => ((y + x) % 2) == 0,
        1 => (y % 2) == 0,
        2 => (x % 3) == 0,
        3 => ((y + x) % 3) == 0,
        4 => (((y / 2) + (x / 3)) % 2) == 0,
        5 => (((y * x) % 2) + ((y * x) % 3)) == 0,
        6 => ((((y * x) % 2) + ((y * x) % 3)) % 2) == 0,
        7 => ((((y + x) % 2) + ((y * x) % 3)) % 2) == 0,
        _ => false,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_mask_counts_v1() {
        // количество служебных модулей (должно быть 233) и data-модулей (208)
        let mut func = 0usize;
        for y in 0..N1 {
            for x in 0..N1 {
                if is_function_v1(x, y) {
                    func += 1;
                }
            }
        }
        let data = N1 * N1 - func;
        assert_eq!(func, 233, "function modules count");
        assert_eq!(data, 208, "data modules count");
    }

    #[test]
    fn walk_pairs_basic_properties() {
        let path = walk_pairs_v1();

        // 1) длина: вся сетка (включая timing-колонку x=6)
        assert_eq!(path.len(), N1 * N1);

        // 2) нет дубликатов; координаты валидны
        let mut seen = vec![false; N1 * N1];
        for &(x, y) in &path {
            assert!(x < N1 && y < N1, "out of bounds: ({x},{y})");
            let idx = y * N1 + x;
            assert!(!seen[idx], "duplicate coord in path: ({x},{y})");
            seen[idx] = true;
        }

        // 3) правый нижний модуль идёт первым
        assert_eq!(path[0], (N1 - 1, N1 - 1));
    }
}