//! ultracode: быстрый детектор штрих-кодов (без зависимостей).
//!
//! Сейчас поддерживается: EAN-13 и UPC-A (в рамках EAN-13).
//! Вход: 8-битное изображение (градации серого) как непрерывный буфер row-major.
//!
//! Дальше план: QR (finder + perspective + Reed-Solomon), Code128, Code39.
//!
//! Без внешних зависимостей, только std.

mod binarize;
mod one_d;
pub mod qr;

pub use one_d::{decode_ean13_upca, Barcode, BarcodeFormat, DecodeOptions};

/// Простая обёртка над сырым буфером серого изображения.
/// data.len() == width * height
#[derive(Clone, Debug)]
pub struct GrayImage<'a> {
    pub width: usize,
    pub height: usize,
    pub data: &'a [u8],
}

impl<'a> GrayImage<'a> {
    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.data[y * self.width + x]
    }

    pub fn row(&self, y: usize) -> &[u8] {
        let start = y * self.width;
        &self.data[start..start + self.width]
    }
}

/// Высокоуровневое API: попытаться распознать любые известные форматы.
/// Пока вызывает только EAN-13/UPC-A.
pub fn decode_any(img: GrayImage<'_>, opts: DecodeOptions) -> Vec<Barcode> {
    let mut out = Vec::new();
    out.extend(decode_ean13_upca(&img, &opts));
    // TODO: сюда добавить другие форматы по мере реализации.
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Тестовый синтетический ряд с EAN-13 «5901234123457» (валидный контроль).
    // Это не полноценная картинка, а узкая "полоса" с идеальными барами для smoke-теста.
    #[test]
    fn smoke_synthetic_ean13_row() {
        // Сгенерируем идеальный ряд 1xN: чёрное=0, белое=255.
        // Ширины модулей подберём примитивно: unit=2 пикселя.
        let code = "5901234123457";
        let row = crate::one_d::ean13::synthesize_ideal_row(code, 2);
        let img = GrayImage { width: row.len(), height: 1, data: &row };
        let opts = DecodeOptions::default();
        let res = decode_ean13_upca(&img, &opts);
        assert!(!res.is_empty());
        assert_eq!(res[0].text, "5901234123457");
        assert_eq!(res[0].format, BarcodeFormat::EAN13);
    }
}
