//! Быстрая бинаризация и измерение ширин баров на строке.
//!
//! Добавлено:
//! - адаптивная бинаризация по скользящему среднему (без зависимостей),
//!   работает лучше на неравномерной засветке;
//! - прежняя глобальная бинаризация оставлена как фоллбэк.

/// Простой глобальный порог: смесь среднего и середины между min/max.
/// Быстро и без аллокаций, но не любит градиенты освещения.
#[inline]
pub fn otsu_like_threshold(row: &[u8]) -> u8 {
    let (mut min_v, mut max_v) = (u8::MAX, 0u8);
    let mut sum: u64 = 0;
    for &v in row {
        if v < min_v { min_v = v; }
        if v > max_v { max_v = v; }
        sum += v as u64;
    }
    let mean = (sum / row.len() as u64) as u8;
    let mid = ((min_v as u16 + max_v as u16) / 2) as u8;
    ((mean as u16 * 1 + mid as u16 * 1) / 2) as u8
}

/// Глобальная бинаризация строки: true=чёрный, false=белый.
pub fn binarize_row(row: &[u8]) -> Vec<bool> {
    let t = otsu_like_threshold(row);
    row.iter().map(|&v| v < t).collect()
}

/// Адаптивная бинаризация по скользящему среднему окна `win` с небольшим смещением `bias`.
/// Хорошо работает при неравномерной подсветке.
/// Выбор окна: по умолчанию width/32, в диапазоне [8..64].
pub fn binarize_row_adaptive(row: &[u8]) -> Vec<bool> {
    let n = row.len();
    if n == 0 { return Vec::new(); }
    let mut win = n / 32;
    if win < 8 { win = 8; }
    if win > 64 { win = 64; }
    let bias: i32 = 5; // небольшой «запас» в сторону чёрного

    // префиксные суммы для среднего по окну
    let mut pref: Vec<u32> = Vec::with_capacity(n + 1);
    pref.push(0);
    for &v in row {
        let last = *pref.last().unwrap();
        pref.push(last + v as u32);
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let left = i.saturating_sub(win);
        let right = (i + win).min(n - 1);
        let len = (right - left + 1) as u32;
        let sum = pref[right + 1] - pref[left];
        let mean = (sum / len) as i32;
        let v = row[i] as i32;
        out.push(v < mean - bias);
    }
    out
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
    // Оценим базовый модуль как нижний квартиль (устойчиво к толстой правой «лапе»).
    let mut sorted = rl.to_vec();
    sorted.sort_unstable();
    let base = sorted[sorted.len() / 4].max(1);
    let mut modules = Vec::with_capacity(rl.len());
    for &w in rl {
        let m = ((w + base / 2) / base).clamp(1, 4); // EAN использует ширины 1..4
        modules.push(m as u8);
    }
    (modules, row_bin[0]) // если true, первый run — чёрный
}
