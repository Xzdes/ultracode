//! Модуль QR (v1): чтение двух копий 15-битного формат-инфо из матрицы,
//! упаковка битов и выбор лучшего кандидата по расстоянию Хэмминга.

pub mod format;
pub mod data;
pub mod rs;
pub mod encode;

use self::format::{decode_format_word, EcLevel, FORMAT_READ_PATHS_V1};

/// Преобразует u16 в массив из 15 булевых (MSB первым).
#[allow(dead_code)]
fn u16_to_15bits_msb_first(word: u16) -> [bool; 15] {
    let mut out = [false; 15];
    for i in 0..15 {
        // Берём биты с позиций 14..0 (то есть 15 старших разрядов нашей «15-битной шины»)
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

/// Кандидат формата, найденный в одной из двух копий.
#[derive(Copy, Clone, Debug)]
struct FormatCandidate {
    ec: EcLevel,
    mask_id: u8,
    distance: u32,
    source_idx: usize, // 0 или 1 — из какой дорожки
}

/// Основная функция: читает две 15-битные дорожки формата и пытается декодировать.
///
/// Возвращает Some((уровень, id маски, расстояние, индекс источника))
/// при успешном декодировании (расстояние ≤ 3), иначе None.
pub fn decode_v1_format_from_matrix(matrix: &[Vec<bool>]) -> Option<(EcLevel, u8, u32, usize)> {
    // На входе ожидаем матрицу не менее 21×21 (v1).
    if matrix.len() < 21 || matrix[0].len() < 21 {
        return None;
    }

    let [path_a, path_b] = FORMAT_READ_PATHS_V1;

    let w_a = read_15_from_path(matrix, &path_a);
    let w_b = read_15_from_path(matrix, &path_b);

    let mut candidates: Vec<FormatCandidate> = Vec::new();

    if let Some((ec, mask, d)) = decode_format_word(w_a) {
        candidates.push(FormatCandidate {
            ec,
            mask_id: mask,
            distance: d,
            source_idx: 0,
        });
    }
    if let Some((ec, mask, d)) = decode_format_word(w_b) {
        candidates.push(FormatCandidate {
            ec,
            mask_id: mask,
            distance: d,
            source_idx: 1,
        });
    }

    // Если обе копии валидны — выбираем с меньшим расстоянием.
    candidates
        .into_iter()
        .min_by_key(|c| c.distance)
        .map(|c| (c.ec, c.mask_id, c.distance, c.source_idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u16_to_15bits_len_and_order() {
        // MSB среди 15 бит — это разряд 14 (значение 1<<14 = 0x4000).
        let w: u16 = 1 << 14;
        let bits = u16_to_15bits_msb_first(w);
        assert_eq!(bits.len(), 15);
        assert!(bits[0], "ожидался установленный старший бит (index 0)");

        // LSB среди 15 бит — это разряд 0 (значение 1).
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

        // И в пределах 21×21
        for &(x, y) in a.iter().chain(b.iter()) {
            assert!(x < 21 && y < 21, "({x},{y}) out of bounds");
        }
    }
}
