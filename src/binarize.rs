//! Быстрая бинаризация и измерение ширин баров на строке.
//!
//! Добавлено:
//! - адаптивная бинаризация по скользящему среднему (без зависимостей),
//!   работает лучше на неравномерной засветке;
//! - глобальная бинаризация оставлена как фоллбэк.
//!
//! Интерфейс под 1D-сканеры (Code128/EAN-13/UPC):
//! - `binarize_row(&[u8]) -> Vec<bool>`
//! - `binarize_row_adaptive(&[u8]) -> Vec<bool>`
//! - `runs(&[bool]) -> Vec<usize>`
//! - `normalize_modules(&[bool], &[usize]) -> (Vec<u8>, bool)`

/// Простой «Otsu-like» порог: среднее и середина (min+max)/2.
#[inline]
pub fn otsu_like_threshold(row: &[u8]) -> u8 {
    if row.is_empty() { return 0; }
    let (mut min_v, mut max_v) = (u8::MAX, 0u8);
    let mut sum: u64 = 0;
    for &v in row {
        if v < min_v { min_v = v; }
        if v > max_v { max_v = v; }
        sum += v as u64;
    }
    let mean = (sum / row.len() as u64) as u8;
    let mid = ((min_v as u16 + max_v as u16) / 2) as u8;
    ((mean as u16 + mid as u16) / 2) as u8
}

/// Глобальная бинаризация строки: true = чёрный, false = белый.
pub fn binarize_row(row: &[u8]) -> Vec<bool> {
    let t = otsu_like_threshold(row);
    row.iter().map(|&v| v < t).collect()
}

/// Адаптивная бинаризация по скользящему среднему.
/// Окно подбирается от width/32 и ограничивается в [8..64],
/// небольшой `bias` смещает порог в «чёрную» сторону.
pub fn binarize_row_adaptive(row: &[u8]) -> Vec<bool> {
    let n = row.len();
    if n == 0 { return Vec::new(); }

    let mut win = n / 32;
    if win < 8 { win = 8; }
    if win > 64 { win = 64; }
    let bias: i32 = 5;

    // prefix sums
    let mut pref: Vec<u32> = Vec::with_capacity(n + 1);
    pref.push(0);
    for &v in row {
        pref.push(pref.last().unwrap() + v as u32);
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

/// Превратить бинарную строку (true=чёрный) в run-lengths (ширины подряд идущих баров/пробелов).
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

/// Нормализовать run-lengths в условные «модули» (1..4).
/// Возвращает `(вектор_модулей, starts_black)`.
///
/// Алгоритм:
/// 1) Оценить базовый модуль как медиану нижней половины run-ов (устойчив к «толстой лапе»).
/// 2) Квантовать каждую ширину в 1..4 округлением к ближайшему целому.
pub fn normalize_modules(row_bin: &[bool], rl: &[usize]) -> (Vec<u8>, bool) {
    if rl.is_empty() { return (Vec::new(), false); }

    // Базовый модуль — «тонкие» полосы (нижняя половина).
    let mut sorted = rl.to_vec();
    sorted.sort_unstable();
    let thin_slice = &sorted[..(sorted.len().max(1) + 1) / 2];
    let base = {
        let mid = thin_slice.len() / 2;
        if thin_slice.is_empty() {
            1.0f32
        } else if thin_slice.len() % 2 == 1 {
            thin_slice[mid] as f32
        } else {
            (thin_slice[mid - 1] as f32 + thin_slice[mid] as f32) * 0.5
        }.max(1.0)
    };

    let mut mods: Vec<u8> = Vec::with_capacity(rl.len());
    for &w in rl {
        let q = (w as f32 / base).round() as i32;
        let q = q.clamp(1, 4) as u8;
        mods.push(q);
    }

    let starts_black = row_bin.first().copied().unwrap_or(false);
    (mods, starts_black)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otsu_like_threshold_basic() {
        let row = [10u8, 12, 15, 240, 250];
        let t = otsu_like_threshold(&row);
        assert!(t > 30 && t < 200);
    }

    #[test]
    fn binarize_shapes_runs() {
        let row = [255u8,255, 0,0,0, 255, 0,0];
        let b = binarize_row(&row);
        let r = runs(&b);
        assert!(!r.is_empty());
    }

    #[test]
    fn normalize_simple() {
        let row_bin = [true,false,true,false,true];
        let rl = [1usize,1,3,1,1];
        let (mods, starts_black) = normalize_modules(&row_bin, &rl);
        assert_eq!(mods.len(), rl.len());
        assert_eq!(starts_black, true);
    }
}
