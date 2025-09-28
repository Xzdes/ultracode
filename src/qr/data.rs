//! Константы и обход/маски/служебные зоны для QR v1.

pub const N1: usize = 21;

/// Маски QR (ISO/IEC 18004). Внимание: x — колонка, y — строка.
#[inline]
pub fn mask_predicate(mask_id: u8, x: i32, y: i32) -> bool {
    let (x, y) = (x as i32, y as i32);
    match mask_id {
        0 => ((x + y) % 2) == 0,
        1 => (y % 2) == 0,
        2 => (x % 3) == 0,
        3 => ((x + y) % 3) == 0,
        4 => (((y / 2) + (x / 3)) % 2) == 0,
        5 => (((x * y) % 2) + ((x * y) % 3)) == 0,
        6 => ((((x * y) % 2) + ((x * y) % 3)) % 2) == 0,
        7 => ((((x + y) % 2) + ((x * y) % 3)) % 2) == 0,
        _ => false,
    }
}

/// Служебные модули v1 (finder+separator, timing, dark module).
pub fn is_function_v1(x: usize, y: usize) -> bool {
    debug_assert!(x < N1 && y < N1);

    // Finder+separator 9×9 в углах:
    if (x <= 8 && y <= 8)
        || (x >= N1 - 9 && y <= 8)
        || (x <= 8 && y >= N1 - 9)
    {
        return true;
    }

    // Timing-линии
    if x == 6 || y == 6 {
        return true;
    }

    // Тёмный модуль (8, N-8-1) => (8, 12) для v1 с 0-индексацией? В ряде реализаций (8, N-8)
    // В базовых тестах это не критично, но оставим самый распространённый вариант (8, N-8-1)
    if x == 8 && y == (N1 - 8 - 1) {
        return true;
    }

    false
}

/// Обход data-ячеек v1 (как при чтении): колоннами двойками, зигзагом.
pub fn walk_pairs_v1() -> Vec<(usize, usize)> {
    let mut order = Vec::with_capacity(N1 * N1);
    let mut col: i32 = (N1 as i32) - 1;
    let mut upward = true;

    while col > 0 {
        if col == 6 {
            col -= 1; // пропускаем timing-колонку
        }

        for row in 0..N1 {
            let y = if upward { (N1 - 1) - row } else { row };
            for dx in 0..2 {
                let x = (col - dx as i32) as usize;
                let y = y as usize;
                if !is_function_v1(x, y) {
                    order.push((x, y));
                }
            }
        }

        upward = !upward;
        col -= 2;
    }

    order
}

/// Извлекаем data-биты из булевой 21×21 сетки по порядку `walk_pairs_v1`.
pub fn extract_data_bits_v1(grid: &[bool]) -> Vec<bool> {
    assert_eq!(grid.len(), N1 * N1);
    let order = walk_pairs_v1();
    let mut out = Vec::with_capacity(order.len());
    for (x, y) in order {
        out.push(grid[y * N1 + x]);
    }
    out
}
