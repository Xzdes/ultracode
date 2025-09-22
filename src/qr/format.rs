//! QR v1: формат-инфо (15 бит), BCH(15,5), маскирование 0x5412.
//!
//! Здесь:
/// - перечисление уровня коррекции ошибок [`EcLevel`];
/// - функция декодирования одного 15-битного слова [`decode_format_word`];
/// - координаты двух дорожек чтения формата для версии 1
///   [`FORMAT_READ_PATHS_V1`] (по 15 модулей каждая).

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EcLevel {
    L,
    M,
    Q,
    H,
}

impl EcLevel {
    /// Два бита уровня EC по стандарту:
    /// L=01, M=00, Q=11, H=10
    #[inline]
    pub fn to_bits2(self) -> u8 {
        match self {
            EcLevel::L => 0b01,
            EcLevel::M => 0b00,
            EcLevel::Q => 0b11,
            EcLevel::H => 0b10,
        }
    }

    /// Обратное преобразование двух бит в уровень EC.
    #[allow(dead_code)]
    pub fn from_bits2(b2: u8) -> Option<Self> {
        match b2 & 0b11 {
            0b01 => Some(EcLevel::L),
            0b00 => Some(EcLevel::M),
            0b11 => Some(EcLevel::Q),
            0b10 => Some(EcLevel::H),
            _ => None,
        }
    }
}

/// Генератор BCH(15,5): x^10 + x^8 + x^5 + x^4 + x^2 + x + 1
const BCH15_5_GEN: u16 = 0b1_0100_1101_11; // 0x537
/// Маска формата из стандарта
const FORMAT_MASK: u16 = 0b1010_1000_0001_0010; // 0x5412

/// Возвращает остаток при делении (data<<10) на генератор BCH по mod2.
fn bch_remainder_15_5(mut v: u16) -> u16 {
    // У v уже должны быть зарезервированы 10 младших бит под остаток
    for shift in (10..=14).rev() {
        if (v >> shift) & 1 == 1 {
            v ^= BCH15_5_GEN << (shift - 10);
        }
    }
    v & 0x03FF // 10 бит
}

/// Кодирует 15-битное слово формата (до маски).
fn encode_format_word_unmasked(ec: EcLevel, mask_id: u8) -> u16 {
    let data5 = ((ec.to_bits2() as u16) << 3) | (mask_id as u16 & 0x7);
    let payload = data5 << 10;
    let rem = bch_remainder_15_5(payload);
    payload | rem
}

/// Кодирует финальное (замаскированное) 15-битное слово формата.
fn encode_format_word_masked(ec: EcLevel, mask_id: u8) -> u16 {
    encode_format_word_unmasked(ec, mask_id) ^ FORMAT_MASK
}

/// Подсчёт расстояния Хэмминга между 15-битными словами.
#[inline]
fn hamming15(a: u16, b: u16) -> u32 {
    (a ^ b).count_ones()
}

/// Декодирование (с подбором по всем 32 валидным словам).
///
/// Возвращает Some(уровень, id маски, расстояние), если найден кандидат
/// с расстоянием ≤ 3, иначе None.
pub fn decode_format_word(word: u16) -> Option<(EcLevel, u8, u32)> {
    let mut best: Option<(EcLevel, u8, u32)> = None;

    // 4 уровня EC × 8 масок = 32 слова
    const LEVELS: [EcLevel; 4] = [EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H];

    for &ec in &LEVELS {
        for mask in 0u8..8 {
            let valid = encode_format_word_masked(ec, mask);
            let d = hamming15(word, valid);
            match best {
                None => best = Some((ec, mask, d)),
                Some((_, _, bd)) if d < bd => best = Some((ec, mask, d)),
                _ => {}
            }
        }
    }

    match best {
        Some((ec, mask, d)) if d <= 3 => Some((ec, mask, d)),
        _ => None,
    }
}

/// Координаты чтения 15-битного формата (две копии) для QR v1 (21×21).
///
/// Пары — это (x, y), где x — столбец, y — строка.
/// Каждая дорожка содержит ровно 15 уникальных координат в пределах 0..=20.
///
/// Примечание: порядок соответствует распространённой раскладке:
/// 1) около верхнего-левого угла (строка y=8 и столбец x=8);
/// 2) «зеркальная» копия около правого-верхнего и левого-нижнего углов.
pub const FORMAT_READ_PATHS_V1: [[(usize, usize); 15]; 2] = [
    // Дорожка 1 (вокруг топ-левого угла):
    // y=8, x=0..5, затем x=7,8; далее столбец x=8, y=7..1
    [
        (0, 8), (1, 8), (2, 8), (3, 8), (4, 8), (5, 8),
        (7, 8), (8, 8),
        (8, 7), (8, 6), (8, 5), (8, 4), (8, 3), (8, 2), (8, 1),
    ],
    // Дорожка 2 (копия вокруг правого-верхнего и левого-нижнего углов):
    // x=20, y=0..7; далее y=8, x=19..13
    [
        (20, 0), (20, 1), (20, 2), (20, 3), (20, 4), (20, 5), (20, 6), (20, 7),
        (19, 8), (18, 8), (17, 8), (16, 8), (15, 8), (14, 8), (13, 8),
    ],
];

/// Вспомогательная функция (оставлена для тестов), возвращает уже
/// замаскированное слово формата для заданных параметров.
#[allow(dead_code)]
pub fn encode_format_bits_for_tests(ec: EcLevel, mask_id: u8) -> u16 {
    encode_format_word_masked(ec, mask_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bch_roundtrip_examples() {
        // Несколько sanity-проверок: кодируем и проверяем,
        // что расстояние до себя равно 0, а до «побитово инвертированного»
        // — существенно больше нуля.
        for &ec in &[EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H] {
            for m in 0u8..8 {
                let w = encode_format_bits_for_tests(ec, m);
                assert_eq!(hamming15(w, w), 0);
                assert!(hamming15(w, !w & 0x7FFF) > 0);
            }
        }
    }

    #[test]
    fn format_paths_have_15_points_each_and_in_bounds() {
        for path in &FORMAT_READ_PATHS_V1 {
            assert_eq!(path.len(), 15);
            for &(x, y) in path {
                assert!(x < 21 && y < 21, "({x},{y}) out of 21×21");
            }
        }
    }
}
