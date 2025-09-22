//! Code 128: декодер по одной строке + синтезатор (для тестов/демо).
//!
//! Поддержка:
//! - Наборы A/B/C, коды CODE A/B/C, SHIFT, FNC1 (ASCII 29, GS).
//! - Проверка checksum (mod 103).
//! - Детект всех трёх старт-кодов + STOP.
//!
//! Ключевая идея: ищем STOP-паттерн (7 run'ов, сумма 13), а затем
//! идём НАЗАД по 6-run блокам до старт-кода. Это надёжно выравнивает
//! поток и убирает двусмысленности «с какого run'а начинать».

use crate::binarize::{binarize_row, binarize_row_adaptive, runs};
use crate::one_d::DecodeOptions;

/// Паттерны 0..=105: по 6 чисел (bars/spaces), сумма 11.
const CODE128_PATTERNS_STR: [&str; 106] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212",
    "221213", "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221",
    "223211", "221132", "221231", "213212", "223112", "312131", "311222", "321122", "321221",
    "312212", "322112", "322211", "212123", "212321", "232121", "111323", "131123", "131321",
    "112313", "132113", "132311", "211313", "231113", "231311", "112133", "112331", "132131",
    "113123", "113321", "133121", "313121", "211331", "231131", "213113", "213311", "213131",
    "311123", "311321", "331121", "312113", "312311", "332111", "314111", "221411", "431111",
    "111224", "111422", "121124", "121421", "141122", "141221", "112214", "112412", "122114",
    "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111", "111242",
    "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
    "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311",
    "113141", "114131", "311141", "411131", "211412", "211214",
    "211232", // 103..105 = Start A/B/C
];

/// STOP-паттерн (7 чисел, сумма 13).
const CODE128_STOP: [u8; 7] = [2, 3, 3, 1, 1, 1, 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeSet {
    A,
    B,
    C,
}

/// Попытка декодировать один ряд в Code128. Успех -> строка.
pub fn decode_row(row_gray: &[u8], opts: &DecodeOptions) -> Option<String> {
    if row_gray.len() < opts.min_modules {
        return None;
    }

    // 1) бинаризация (адаптивная -> фоллбэк) и run-lengths
    let rb1 = binarize_row_adaptive(row_gray);
    let rl1 = runs(&rb1);
    let rl = if rl1.len() >= 24 {
        rl1
    } else {
        let rb2 = binarize_row(row_gray);
        let rl2 = runs(&rb2);
        if rl2.len() < 24 {
            return None;
        }
        rl2
    };

    let patterns = get_patterns();

    // 2) ищем STOP: окно из 7 run'ов нормализуем к сумме 13 и сравниваем
    let mut stop_pos: Option<usize> = None;
    for i in 0..=rl.len().saturating_sub(7) {
        let cand = normalize7(&rl[i..i + 7]);
        if patdist7(cand, CODE128_STOP) <= 1 {
            stop_pos = Some(i);
            break;
        }
    }
    let stop_i = stop_pos?; // индекс первого run'а STOP

    // 3) идём НАЗАД по 6-run символам, пока не встретим Start A/B/C
    let mut idx = stop_i; // текущая правая граница символа
    if idx < 6 {
        return None;
    }

    let mut vals_rev: Vec<u8> = Vec::new(); // чек + payload (в обратном порядке справа-налево)
    let mut start_set: Option<CodeSet> = None;

    while idx >= 6 {
        let pat6 = normalize6(&rl[idx - 6..idx]);
        let (val, dist) = best_code_match(pat6, &patterns);
        if dist > 1 || val > 105 {
            return None;
        }
        if (103..=105).contains(&val) {
            start_set = Some(match val {
                103 => CodeSet::A,
                104 => CodeSet::B,
                105 => CodeSet::C,
                _ => unreachable!(),
            });
            break;
        } else {
            vals_rev.push(val as u8);
            idx -= 6;
            if idx < 6 {
                break;
            }
        }
    }

    let start_set = start_set?;
    if vals_rev.is_empty() {
        return None;
    } // нет даже checksum

    // в прямой порядок: [payload..., checksum]
    vals_rev.reverse();
    let values = vals_rev;

    // 4) проверим checksum (ВАЖНО: считаем ТОЛЬКО по payload, без последнего символа)
    let n = values.len() - 1; // длина payload
    let mut sum = match start_set {
        CodeSet::A => 103u32,
        CodeSet::B => 104u32,
        CodeSet::C => 105u32,
    };
    for (i, &v) in values[..n].iter().enumerate() {
        sum = sum.wrapping_add((v as u32) * ((i as u32) + 1));
    }
    let checksum_calc = sum % 103;
    let checksum_from_code = values[n] as u32;
    if checksum_calc != checksum_from_code {
        return None;
    }

    // 5) декодируем payload (без checksum) начиная с ИСХОДНОГО старт-набора
    let payload = &values[..n];
    decode_values_to_text(payload, start_set)
}

// === Локальная нормализация символов ===

#[inline]
fn normalize6(slice: &[usize]) -> [u8; 6] {
    debug_assert!(slice.len() == 6);
    let sum: usize = slice.iter().sum();
    let scale = (sum as f32) / 11.0_f32;
    let mut out = [0u8; 6];
    for (k, &w) in slice.iter().enumerate() {
        let v = ((w as f32) / scale).round() as i32;
        out[k] = v.clamp(1, 4) as u8;
    }
    adjust_sum_to(&mut out, 11);
    out
}

#[inline]
fn normalize7(slice: &[usize]) -> [u8; 7] {
    debug_assert!(slice.len() == 7);
    let sum: usize = slice.iter().sum();
    let scale = (sum as f32) / 13.0_f32;
    let mut out = [0u8; 7];
    for (k, &w) in slice.iter().enumerate() {
        let v = ((w as f32) / scale).round() as i32;
        out[k] = v.clamp(1, 4) as u8; // в STOP максимум 3, но clamp(4) безопасен
    }
    adjust_sum_to7(&mut out, 13);
    out
}

fn adjust_sum_to(v: &mut [u8; 6], target: i32) {
    let mut sum: i32 = v.iter().map(|&x| x as i32).sum();
    while sum != target {
        if sum > target {
            if let Some((i, _)) = v.iter().enumerate().rev().max_by_key(|(_, &x)| x) {
                if v[i] > 1 {
                    v[i] -= 1;
                    sum -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        } else {
            if let Some((i, _)) = v.iter().enumerate().min_by_key(|(_, &x)| x) {
                if v[i] < 4 {
                    v[i] += 1;
                    sum += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
}

fn adjust_sum_to7(v: &mut [u8; 7], target: i32) {
    let mut sum: i32 = v.iter().map(|&x| x as i32).sum();
    while sum != target {
        if sum > target {
            if let Some((i, _)) = v.iter().enumerate().rev().max_by_key(|(_, &x)| x) {
                if v[i] > 1 {
                    v[i] -= 1;
                    sum -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        } else {
            if let Some((i, _)) = v.iter().enumerate().min_by_key(|(_, &x)| x) {
                if v[i] < 4 {
                    v[i] += 1;
                    sum += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
}

// === Декодирование код-значений в текст ===

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum NextShift {
    None,
    A,
    B,
}

fn decode_values_to_text(vals: &[u8], mut set: CodeSet) -> Option<String> {
    let mut out = String::new();
    let mut i = 0usize;
    let mut shift: NextShift = NextShift::None;

    while i < vals.len() {
        let v = vals[i] as u32;

        // SHIFT действует на один следующий символ
        let effective_set = match (set, shift) {
            (CodeSet::A, NextShift::B) => CodeSet::B,
            (CodeSet::B, NextShift::A) => CodeSet::A,
            _ => set,
        };

        match effective_set {
            CodeSet::A => match v {
                0..=95 => out.push(v as u8 as char), // ASCII 0..95
                96 | 97 => {}                        // FNC3/FNC2 — пропустим
                98 => { /* SHIFT — применится к следующему */ }
                99 => set = CodeSet::C,
                100 => set = CodeSet::B,
                101 => { /* остаёмся в A */ }
                102 => out.push(29u8 as char), // FNC1 -> ASCII GS
                _ => return None,
            },
            CodeSet::B => match v {
                0..=95 => out.push((v as u8 + 32) as char), // ASCII 32..127
                96 | 97 => {}
                98 => { /* SHIFT — применится к следующему */ }
                99 => set = CodeSet::C,
                100 => { /* остаёмся в B */ }
                101 => set = CodeSet::A,
                102 => out.push(29u8 as char),
                _ => return None,
            },
            CodeSet::C => match v {
                99 => { /* CODE C — остаёмся в C */ }
                0..=98 => {
                    // две цифры за символ
                    out.push(char::from(b'0' + (v / 10) as u8));
                    out.push(char::from(b'0' + (v % 10) as u8));
                }
                100 => set = CodeSet::B,
                101 => set = CodeSet::A,
                102 => out.push(29u8 as char),
                _ => return None,
            },
        }

        if shift != NextShift::None {
            shift = NextShift::None;
        } else if v == 98 {
            shift = match set {
                CodeSet::A => NextShift::B,
                CodeSet::B => NextShift::A,
                CodeSet::C => NextShift::None, // в C shift не применим
            };
        }

        i += 1;
    }
    Some(out)
}

// === Паттерны и сопоставление ===

#[inline]
fn get_patterns() -> [[u8; 6]; 106] {
    let mut out = [[0u8; 6]; 106];
    for (i, s) in CODE128_PATTERNS_STR.iter().enumerate() {
        let b = s.as_bytes();
        out[i] = [
            (b[0] - b'0'),
            (b[1] - b'0'),
            (b[2] - b'0'),
            (b[3] - b'0'),
            (b[4] - b'0'),
            (b[5] - b'0'),
        ];
    }
    out
}

#[inline]
fn patdist6(p: [u8; 6], q: [u8; 6]) -> u32 {
    (p[0] as i32 - q[0] as i32).abs() as u32
        + (p[1] as i32 - q[1] as i32).abs() as u32
        + (p[2] as i32 - q[2] as i32).abs() as u32
        + (p[3] as i32 - q[3] as i32).abs() as u32
        + (p[4] as i32 - q[4] as i32).abs() as u32
        + (p[5] as i32 - q[5] as i32).abs() as u32
}

#[inline]
fn patdist7(p: [u8; 7], q: [u8; 7]) -> u32 {
    (p[0] as i32 - q[0] as i32).abs() as u32
        + (p[1] as i32 - q[1] as i32).abs() as u32
        + (p[2] as i32 - q[2] as i32).abs() as u32
        + (p[3] as i32 - q[3] as i32).abs() as u32
        + (p[4] as i32 - q[4] as i32).abs() as u32
        + (p[5] as i32 - q[5] as i32).abs() as u32
        + (p[6] as i32 - q[6] as i32).abs() as u32
}

fn best_code_match(pat: [u8; 6], patterns: &[[u8; 6]; 106]) -> (usize, u32) {
    let mut best = (u32::MAX, 0usize);
    for (i, q) in patterns.iter().enumerate() {
        let d = patdist6(pat, *q);
        if d < best.0 {
            best = (d, i);
        }
        if best.0 == 0 {
            break;
        }
    }
    (best.1, best.0)
}

// === Синтезатор для тестов/демо ===

/// Сгенерировать идеальный одномерный ряд (ч/б пиксели) для Code128.
/// Поддержка наборов: 'A', 'B', 'C'.
pub fn synthesize_row_code128(text: &str, set: char, unit: usize) -> Vec<u8> {
    assert!(unit >= 1);
    let patterns = get_patterns();

    // 1) собрать последовательность кодов (без checksum/stop)
    let mut codes: Vec<usize> = Vec::new();
    let set_cur = match set {
        'A' | 'a' => CodeSet::A,
        'B' | 'b' => CodeSet::B,
        'C' | 'c' => CodeSet::C,
        _ => CodeSet::B,
    };

    match set_cur {
        CodeSet::A => codes.push(103),
        CodeSet::B => codes.push(104),
        CodeSet::C => codes.push(105),
    }

    match set_cur {
        CodeSet::B => {
            for ch in text.chars() {
                let b = ch as u32;
                assert!((32..=127).contains(&b), "Code128B: только ASCII 32..127");
                codes.push((b - 32) as usize);
            }
        }
        CodeSet::A => {
            for ch in text.chars() {
                let b = ch as u32;
                assert!((0..=95).contains(&b), "Code128A: только ASCII 0..95");
                codes.push(b as usize);
            }
        }
        CodeSet::C => {
            assert!(
                text.len() % 2 == 0,
                "Code128C: число цифр должно быть чётным"
            );
            let bytes = text.as_bytes();
            for k in (0..bytes.len()).step_by(2) {
                assert!(
                    bytes[k].is_ascii_digit() && bytes[k + 1].is_ascii_digit(),
                    "Code128C: только цифры"
                );
                let v = (bytes[k] - b'0') as usize * 10 + (bytes[k + 1] - b'0') as usize;
                codes.push(v);
            }
        }
    }

    // 2) checksum
    let mut sum = codes[0] as u32;
    for (i, &v) in codes.iter().enumerate().skip(1) {
        sum += (v as u32) * (i as u32);
    }
    let check = (sum % 103) as usize;
    codes.push(check);

    // 3) собрать модули: quiet(10) + символы + STOP + quiet(10)
    let mut modules: Vec<u8> = Vec::new();
    modules.push(10); // quiet белый
    for &code in &codes {
        modules.extend_from_slice(&patterns[code]);
    }
    modules.extend_from_slice(&CODE128_STOP);
    modules.push(10); // quiet белый

    // 4) модули -> пиксели (начинаем с белого — quiet)
    let mut pix: Vec<u8> = Vec::new();
    let mut black = false;
    for m in modules {
        let w = (m as usize) * unit;
        let val = if black { 0 } else { 255 };
        for _ in 0..w {
            pix.push(val);
        }
        black = !black;
    }
    pix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GrayImage;

    #[test]
    fn code128_b_simple() {
        let row = synthesize_row_code128("HELLO-128", 'B', 2);
        let img = GrayImage {
            width: row.len(),
            height: 1,
            data: &row,
        };
        let opts = DecodeOptions::default();
        let res = super::super::decode_code128(&img, &opts);
        assert!(!res.is_empty());
        assert_eq!(res[0].text, "HELLO-128");
    }

    #[test]
    fn code128_c_digits() {
        let row = synthesize_row_code128("0123456789", 'C', 2);
        let img = GrayImage {
            width: row.len(),
            height: 1,
            data: &row,
        };
        let opts = DecodeOptions::default();
        let res = super::super::decode_code128(&img, &opts);
        assert!(!res.is_empty());
        assert_eq!(res[0].text, "0123456789");
    }

    #[test]
    fn code128_b_ascii_span() {
        let row = synthesize_row_code128("ABcd[]", 'B', 2);
        let img = GrayImage {
            width: row.len(),
            height: 1,
            data: &row,
        };
        let opts = DecodeOptions::default();
        let res = super::super::decode_code128(&img, &opts);
        assert!(!res.is_empty());
        assert_eq!(res[0].text, "ABcd[]");
    }
}
