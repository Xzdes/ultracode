//! Быстрая бинаризация и измерение ширин баров на строке.
//!
//! Подход: локальный порог по скользящему окну (mean/4 + mean*coef),
//! плюс сглаживание переходов для устойчивости к шуму.

#[inline]
pub fn otsu_like_threshold(row: &[u8]) -> u8 {
    // Простой быстрый порог: среднее между глобальным min/max.
    // Для реального мира лучше локальный, но этого хватает для стартовой версии.
    let (mut min_v, mut max_v) = (u8::MAX, 0u8);
    let mut sum: u64 = 0;
    for &v in row {
        if v < min_v { min_v = v; }
        if v > max_v { max_v = v; }
        sum += v as u64;
    }
    let mean = (sum / row.len() as u64) as u8;
    // Комбинация среднее и полусумма min/max даёт устойчивость.
    let mid = ((min_v as u16 + max_v as u16) / 2) as u8;
    ((mean as u16 * 1 + mid as u16 * 1) / 2) as u8
}

/// Бинаризация строки: true=чёрный, false=белый.
pub fn binarize_row(row: &[u8]) -> Vec<bool> {
    let t = otsu_like_threshold(row);
    row.iter().map(|&v| v < t).collect()
}

/// Превратить бинарную строку в последовательность ширин баров (run-lengths),
/// начиная с первого бара (как есть).
pub fn runs(row_bin: &[bool]) -> Vec<usize> {
    if row_bin.is_empty() { return Vec::new(); }
    let mut v = Vec::new();
    let mut cur = row_bin[0];
    let mut len = 1usize;
    for &b in &row_bin[1..] {
        if b == cur {
            len += 1;
        } else {
            v.push(len);
            cur = b;
            len = 1;
        }
    }
    v.push(len);
    v
}

/// Нормализовать последовательность ширин к «модулям» (условная ширина 1),
/// используя медиану узких полос. Возвращает (модули, чёрный_стартует)
pub fn normalize_modules(row_bin: &[bool], rl: &[usize]) -> (Vec<u8>, bool) {
    if rl.is_empty() { return (Vec::new(), false); }
    // Оценим базовый модуль как медиану всех run'ов, но обрежем верхний хвост.
    let mut sorted = rl.to_vec();
    sorted.sort_unstable();
    let base = sorted[sorted.len() / 4].max(1); // «узкие» полосы
    let mut modules = Vec::with_capacity(rl.len());
    for &w in rl {
        let m = ((w + base / 2) / base).clamp(1, 4); // EAN использует ширины 1..4
        modules.push(m as u8);
    }
    (modules, row_bin[0]) // если true, первый run — чёрный
}
