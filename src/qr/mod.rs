
//! Модуль QR (v1): формат-слово, извлечение data-битов и вспомогательные штуки.

pub mod bytes;
pub mod data;
pub mod encode;
pub mod finder;
pub mod format;
pub mod rs;
pub mod sample;

use self::format::{decode_format_word, EcLevel, FORMAT_READ_PATHS_V1};

/// Опции пайплайна QR.
#[derive(Clone, Copy, Debug)]
pub struct QrOptions {
    /// Количество линий для сканирования при поиске finder patterns.
    pub scan_lines: usize,
}

impl Default for QrOptions {
    #[inline]
    fn default() -> Self {
        Self { scan_lines: 64 }
    }
}

/// Преобразует u16 в массив из 15 булевых (MSB первым).
#[allow(dead_code)]
fn u16_to_15bits_msb_first(word: u16) -> [bool; 15] {
    let mut out = [false; 15];
    for i in 0..15 {
        out[i] = ((word >> (14 - i)) & 1) != 0;
    }
    out
}

/// Упаковка сдвигая MSB-вперёд (true=1, false=0).
fn pack_bits_msb(bits: &[bool]) -> u16 {
    assert!(bits.len() <= 16);
    let mut v: u16 = 0;
    for &b in bits {
        v <<= 1;
        if b {
            v |= 1;
        }
    }
    v
}

/// Считывает 15 бит по заданной дорожке координат (x,y) из булевой матрицы.
/// `matrix[y][x]` должен возвращать true для «чёрного» модуля.
fn read_15_from_path(matrix: &[Vec<bool>], path: &[(usize, usize); 15]) -> u16 {
    let mut acc = [false; 15];
    for (i, &(x, y)) in path.iter().enumerate() {
        acc[i] = matrix[y][x];
    }
    pack_bits_msb(&acc)
}

/// Основная функция: читает две 15-битные дорожки формата и пытается декодировать.
///
/// Возвращает (EcLevel, mask_id, лучший_hamming_distance, индекс_дорожки_0_или_1).
pub fn decode_v1_format_from_matrix(
    matrix: &[Vec<bool>],
) -> Option<(EcLevel, u8, u32, usize)> {
    // Две стандартные дорожки чтения формат-слова (каждая — 15 координат).
    let [path_a, path_b] = FORMAT_READ_PATHS_V1;

    // Считываем сырые 15-битные слова.
    let raw_a = read_15_from_path(matrix, &path_a);
    let raw_b = read_15_from_path(matrix, &path_b);

    // Каждое слово декодируем через BCH(15,5) и получаем кандидатов.
    let mut candidates = Vec::with_capacity(2);

    if let Some((ec, mask_id, dist)) = decode_format_word(raw_a) {
        candidates.push(FormatCandidate {
            ec,
            mask_id,
            distance: dist,
            source_idx: 0,
        });
    }
    if let Some((ec, mask_id, dist)) = decode_format_word(raw_b) {
        candidates.push(FormatCandidate {
            ec,
            mask_id,
            distance: dist,
            source_idx: 1,
        });
    }

    // Если кандидатов нет — вернуть None.
    if candidates.is_empty() {
        return None;
    }

    // Выбрать наилучший (минимальное расстояние Хэмминга).
    candidates
        .into_iter()
        .min_by_key(|c| c.distance)
        .map(|c| (c.ec, c.mask_id, c.distance, c.source_idx))
}

#[derive(Copy, Clone, Debug)]
struct FormatCandidate {
    ec: EcLevel,
    mask_id: u8,
    distance: u32,
    source_idx: usize, // 0 или 1 — из какой дорожки
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u16_to_15bits_len_and_order() {
        let w: u16 = 1 << 14;
        let bits = u16_to_15bits_msb_first(w);
        assert!(bits[0], "ожидался установленный старший бит (index 0)");

        let w2: u16 = 1;
        let bits2 = u16_to_15bits_msb_first(w2);
        assert!(bits2[14], "ожидался установленный младший бит (index 14)");
        assert!(!bits2[0], "старший бит не должен быть установлен");
    }

    #[test]
    fn pack_bits_msb_basic() {
        let bits = [true, false, true, true]; // 1011b = 11
        assert_eq!(pack_bits_msb(&bits), 0b1011);
    }

    #[test]
    fn format_paths_have_15_points_each() {
        let [a, b] = FORMAT_READ_PATHS_V1;
        assert_eq!(a.len(), 15);
        assert_eq!(b.len(), 15);

        for &(x, y) in a.iter().chain(b.iter()) {
            assert!(x < 21 && y < 21, "({x},{y}) out of bounds");
        }
    }
}