//! QR v1 (21×21): служебные зоны и порядок обхода «парами колонок».
//!
//! Здесь три ключевые вещи:
//! 1) [`is_function_v1`] — пометка служебных модулей (finder+separator, timing,
//!    format и т.п.) — они не несут data/ECC бит.
//! 2) [`walk_pairs_v1`] — правильный маршрут чтения модулей для извлечения бит:
//!    идём парами колонок (x, x-1) справа налево, «змейкой» по y. Колонку x=6
//!    (timing) пропускаем как пару — т.е. после x=7 сразу x=5.
//! 3) [`extract_data_bits_v1`] — снимаем только data-модули (ровно 208 бит для v1).
//!
//! Важно: реализация подобрана так, чтобы общее число служебных модулей было 233,
//! а число data-модулей — 208 (19 data CW + 7 ECC CW = 26 байт = 208 бит).

/// Размер сетки для версии 1.
pub const N1: usize = 21;

/// Является ли модуль служебным (не data/ECC) для QR v1.
///
/// Покрываем:
/// - Finder + белые сепараторы вокруг (три угла): прямоугольники 9×9 / 8×9 / 9×8.
/// - Timing-линии: вся колонка x=6 и вся строка y=6 (кроме зон finder — они уже
///   попадают в прямоугольники).
/// - Формат-поля оказываются внутри этих прямоугольников.
/// - «Тёмный модуль» (dark module) для v1 расположен в левом нижнем блоке,
///   он тоже попадает в соответствующий прямоугольник, поэтому отдельно его
///   не отмечаем.
#[inline]
pub fn is_function_v1(x: usize, y: usize) -> bool {
    debug_assert!(x < N1 && y < N1);

    // Finder+separator прямоугольники:
    //  - левый верхний 9×9: x<=8, y<=8
    //  - правый верхний 8×9: x>=N-8, y<=8
    //  - левый нижний 9×8: x<=8, y>=N-8
    if (x <= 8 && y <= 8) || (x >= N1 - 8 && y <= 8) || (x <= 8 && y >= N1 - 8) {
        return true;
    }

    // Timing-линии (вертикальная и горизонтальная через центр сетки):
    if x == 6 || y == 6 {
        return true;
    }

    false
}

/// Маршрут обхода для выборки бит: пары колонок (x, x-1), справа налево,
/// «змейкой» по y. Пару с x=6 пропускаем (это timing-колонка).
///
/// Возвращает порядок координат ВСЕХ модулей сетки; далее потребитель сам
/// отфильтровывает служебные через [`is_function_v1`].
pub fn walk_pairs_v1() -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(N1 * N1);

    let mut x: isize = (N1 as isize) - 1; // стартуем с x=20
    let mut upward = true;                // первая пара — движение вверх

    while x >= 0 {
        // пропускаем timing-колонку (x=6) как пару
        if x == 6 {
            x -= 1;
            if x < 0 { break; }
        }

        let xx = x as usize;

        if upward {
            for y in (0..N1).rev() {
                out.push((xx, y));
                if xx > 0 {
                    out.push((xx - 1, y));
                }
            }
        } else {
            for y in 0..N1 {
                out.push((xx, y));
                if xx > 0 {
                    out.push((xx - 1, y));
                }
            }
        }

        upward = !upward;
        x -= 2; // к следующей паре колонок
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_mask_counts_v1() {
        // количество служебных модулей (должно быть 233) и data-модулей (208)
        let mut func = 0usize;
        for y in 0..N1 {
            for x in 0..N1 {
                if is_function_v1(x, y) { func += 1; }
            }
        }
        let data = N1 * N1 - func;
        assert_eq!(func, 233, "function modules count");
        assert_eq!(data, 208, "data modules count");
    }

    #[test]
    fn walk_pairs_basic_properties() {
        let path = walk_pairs_v1();
        // Размер должен покрывать всю сетку (каждый модуль один раз в последовательности)
        assert_eq!(path.len(), N1 * N1 * 2 - 1, "последовательность пар векторизуется (каждый шаг пишет 2 координаты, кроме крайних границ)");
        // Проверим, что x=6 как «пара» пропущен: в последовательности не
        // встречаются соседние x=(6,5) или (6,7) как начало пары.
        // (Сам модуль x=6 появится, но как часть соседних прямоугольников; этот
        // тест скорее sanity, чем строгая спецификация.)
        // Поищем первый элемент каждой «пары» (шаги по 2).
        let mut saw_x6_as_first = false;
        for (idx, (x, _y)) in path.iter().enumerate() {
            if idx % 2 == 0 && *x == 6 { saw_x6_as_first = true; break; }
        }
        assert!(!saw_x6_as_first, "timing-колонка не должна быть первой в паре");
    }
}
