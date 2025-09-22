//! Декодер EAN-13/UPC-A по одной строке.
//!
//! Алгоритм (быстрый и без зависимостей):
//! 1) Бинаризуем строку (адаптивно, с фоллбэком на глобально) и строим run-lengths.
//! 2) Нормализуем run'ы в модули (1..4).
//! 3) Ищем стартовый guard (101), затем центральный (01010) и финальный (101).
//! 4) Левую половину декодируем с учётом A/B (B = реверс A), правую — C.
//! 5) Определяем первую цифру по маске A/B, проверяем контрольную сумму.

use crate::binarize::{binarize_row, binarize_row_adaptive, normalize_modules, runs};
use crate::one_d::DecodeOptions;

// A (L) — левые «A»-паттерны (bars/spaces), сумма = 7 модулей
const A_PATTERNS: [(u8, u8, u8, u8); 10] = [
    (3, 2, 1, 1),
    (2, 2, 2, 1),
    (2, 1, 2, 2),
    (1, 4, 1, 1),
    (1, 1, 3, 2),
    (1, 2, 3, 1),
    (1, 1, 1, 4),
    (1, 3, 1, 2),
    (1, 2, 1, 3),
    (3, 1, 1, 2),
];

// B (G) — это реверс A (зеркало по run-ам)
const B_PATTERNS: [(u8, u8, u8, u8); 10] = [
    (1, 1, 2, 3),
    (1, 2, 2, 2),
    (2, 2, 1, 2),
    (1, 1, 4, 1),
    (2, 3, 1, 1),
    (1, 3, 2, 1),
    (4, 1, 1, 1),
    (2, 1, 3, 1),
    (3, 1, 2, 1),
    (2, 1, 1, 3),
];

// C (R) — правая сторона; по ширинам совпадает с A (инверсия цветов не важна для run-ширин)
const C_PATTERNS: [(u8, u8, u8, u8); 10] = A_PATTERNS;

/// Маски для определения первой цифры по типам шести левых цифр (A/B).
/// true = B, false = A
const FIRST_DIGIT_MASKS: [(bool, bool, bool, bool, bool, bool); 10] = [
    (false, false, false, false, false, false), // 0
    (false, false, true, false, true, true),    // 1
    (false, false, true, true, false, true),    // 2
    (false, false, true, true, true, false),    // 3
    (false, true, false, false, true, true),    // 4
    (false, true, true, false, false, true),    // 5
    (false, true, true, true, false, false),    // 6
    (false, true, false, true, false, true),    // 7
    (false, true, false, true, true, false),    // 8
    (false, true, true, false, true, false),    // 9
];

/// Попытка декодировать один ряд. Возвращает строку 13 цифр (EAN) или 12 (UPC-A) при успехе.
pub fn decode_row(row_gray: &[u8], opts: &DecodeOptions) -> Option<String> {
    if row_gray.len() < opts.min_modules {
        return None;
    }

    // --- 1) Бинаризация: пробуем адаптивно, фоллбэк на глобальную
    let (modules, _starts_black) = {
        let rb = binarize_row_adaptive(row_gray);
        let rl = runs(&rb);
        if rl.len() >= 40 {
            normalize_modules(&rb, &rl)
        } else {
            let rb2 = binarize_row(row_gray);
            let rl2 = runs(&rb2);
            if rl2.len() < 40 {
                return None;
            }
            normalize_modules(&rb2, &rl2)
        }
    };

    // --- 2) Поиск стартового guard: первые подряд [1,1,1] в модулях ---
    let i = find_guard_start(&modules)?;
    // сдвигаемся за 3 run-а старта
    let mut idx = i + 3;

    // --- 3) Левая половина: 6 цифр, каждая — 4 run'а ---
    let mut left_digits = [0u8; 6];
    let mut left_is_b = [false; 6];
    for d in 0..6 {
        if idx + 3 >= modules.len() {
            return None;
        }
        let pat = (
            modules[idx],
            modules[idx + 1],
            modules[idx + 2],
            modules[idx + 3],
        );
        let (digit_a, dist_a) = best_match(&pat, &A_PATTERNS);
        let (digit_b, dist_b) = best_match(&pat, &B_PATTERNS);
        if dist_a <= dist_b {
            left_digits[d] = digit_a;
            left_is_b[d] = false;
        } else {
            left_digits[d] = digit_b;
            left_is_b[d] = true;
        }
        idx += 4;
    }

    // --- 4) Центральный guard 01010 => 5 run'ов модулей ---
    if !is_guard_center(&modules, idx) {
        return None;
    }
    idx += 5;

    // --- 5) Правая половина: 6 цифр (C-набор) ---
    let mut right_digits = [0u8; 6];
    for d in 0..6 {
        if idx + 3 >= modules.len() {
            return None;
        }
        let pat = (
            modules[idx],
            modules[idx + 1],
            modules[idx + 2],
            modules[idx + 3],
        );
        let (digit_c, _dist_c) = best_match(&pat, &C_PATTERNS);
        right_digits[d] = digit_c;
        idx += 4;
    }

    // --- 6) Финальный guard 101 ---
    if !is_guard_end(&modules, idx) {
        return None;
    }

    // --- 7) Первая цифра по маске типов A/B ---
    let first = deduce_first_digit(&left_is_b)?;
    let mut digits = [0u8; 13];
    digits[0] = first;
    for k in 0..6 {
        digits[1 + k] = left_digits[k];
    }
    for k in 0..6 {
        digits[7 + k] = right_digits[k];
    }

    // --- 8) Контрольная сумма ---
    if !check_ean13_checksum(&digits) {
        return None;
    }

    // UPC-A — это EAN-13 с ведущим 0.
    let text = if digits[0] == 0 {
        digits[1..13]
            .iter()
            .map(|d| (b'0' + *d) as char)
            .collect::<String>()
    } else {
        digits
            .iter()
            .map(|d| (b'0' + *d) as char)
            .collect::<String>()
    };

    Some(text)
}

fn find_guard_start(m: &[u8]) -> Option<usize> {
    for i in 0..m.len().saturating_sub(2) {
        if m[i] == 1 && m[i + 1] == 1 && m[i + 2] == 1 {
            return Some(i);
        }
    }
    None
}

fn is_guard_center(m: &[u8], i: usize) -> bool {
    i + 4 < m.len() && m[i] == 1 && m[i + 1] == 1 && m[i + 2] == 1 && m[i + 3] == 1 && m[i + 4] == 1
}

fn is_guard_end(m: &[u8], i: usize) -> bool {
    i + 2 < m.len() && m[i] == 1 && m[i + 1] == 1 && m[i + 2] == 1
}

/// подобрать ближайшую цифру по паттерну ширин (манхэттенское расстояние)
fn best_match(pat: &(u8, u8, u8, u8), dict: &[(u8, u8, u8, u8); 10]) -> (u8, u32) {
    let mut best_d = u32::MAX;
    let mut best_i = 0u8;
    for (i, &(a, b, c, d)) in dict.iter().enumerate() {
        let dsum = patdist(*pat, (a, b, c, d));
        if dsum < best_d {
            best_d = dsum;
            best_i = i as u8;
        }
    }
    (best_i, best_d)
}

fn patdist(p: (u8, u8, u8, u8), q: (u8, u8, u8, u8)) -> u32 {
    (p.0 as i32 - q.0 as i32).abs() as u32
        + (p.1 as i32 - q.1 as i32).abs() as u32
        + (p.2 as i32 - q.2 as i32).abs() as u32
        + (p.3 as i32 - q.3 as i32).abs() as u32
}

fn deduce_first_digit(mask_b: &[bool; 6]) -> Option<u8> {
    for (d, mask) in FIRST_DIGIT_MASKS.iter().enumerate() {
        if mask_b[0] == mask.0
            && mask_b[1] == mask.1
            && mask_b[2] == mask.2
            && mask_b[3] == mask.3
            && mask_b[4] == mask.4
            && mask_b[5] == mask.5
        {
            return Some(d as u8);
        }
    }
    None
}

fn check_ean13_checksum(d: &[u8; 13]) -> bool {
    let mut sum = 0u32;
    for i in 0..12 {
        let w = if i % 2 == 0 { 1 } else { 3 };
        sum += d[i] as u32 * w;
    }
    let check = (10 - (sum % 10)) % 10;
    check == d[12] as u32
}

/// Вспомогательная функция для юнит-теста: синтез идеального ряда по строке цифр.
#[cfg(test)]
pub fn synthesize_ideal_row(digits: &str, unit: usize) -> Vec<u8> {
    let mut modules: Vec<u8> = Vec::new();
    modules.extend([9]); // quiet (белое)
    modules.extend([1, 1, 1]); // старт 101

    let ds: Vec<u8> = digits.bytes().map(|c| c - b'0').collect();
    let is_upca = ds.len() == 12;
    let mut ean13 = [0u8; 13];
    if is_upca {
        ean13[0] = 0;
        for i in 0..12 {
            ean13[i + 1] = ds[i];
        }
        // пересчёт checksum
        let mut sum = 0u32;
        for i in 0..12 {
            let w = if i % 2 == 0 { 1 } else { 3 };
            sum += ean13[i] as u32 * w;
        }
        ean13[12] = ((10 - (sum % 10)) % 10) as u8;
    } else {
        for i in 0..13 {
            ean13[i] = ds[i];
        }
    }
    let first = ean13[0] as usize;
    let mask = super::ean13::FIRST_DIGIT_MASKS[first];

    // левая половина: A/B
    for i in 0..6 {
        let d = ean13[1 + i] as usize;
        let (a, b, c, dw) = if mask_at(mask, i) {
            B_PATTERNS[d]
        } else {
            A_PATTERNS[d]
        };
        modules.extend([a, b, c, dw]);
    }
    // центр
    modules.extend([1, 1, 1, 1, 1]);
    // правая половина: C
    for i in 0..6 {
        let d = ean13[7 + i] as usize;
        let (a, b, c, dw) = C_PATTERNS[d];
        modules.extend([a, b, c, dw]);
    }
    // финал и quiet
    modules.extend([1, 1, 1]);
    modules.extend([9]);

    // В пиксели (чёрный=0, белый=255), начиная с белого
    let mut pix: Vec<u8> = Vec::new();
    let mut black = false;
    for m in modules {
        let w = m as usize * unit;
        let val = if black { 0u8 } else { 255u8 };
        for _ in 0..w {
            pix.push(val);
        }
        black = !black;
    }
    pix
}

#[cfg(test)]
fn mask_at(mask: (bool, bool, bool, bool, bool, bool), idx: usize) -> bool {
    match idx {
        0 => mask.0,
        1 => mask.1,
        2 => mask.2,
        3 => mask.3,
        4 => mask.4,
        5 => mask.5,
        _ => false,
    }
}
