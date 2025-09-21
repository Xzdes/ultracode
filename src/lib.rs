//! ultracode: быстрый детектор штрих-кодов (без зависимостей).
//!
//! Поддерживается:
//! - EAN-13 и UPC-A (через EAN-13)
//! - Code 128 (наборы A/B/C, SHIFT, FNC1)
//!
//! Вход: 8-битное изображение (градации серого) как непрерывный буфер row-major.

mod binarize;
mod one_d;
pub mod qr;

pub use one_d::{decode_ean13_upca, decode_code128, Barcode, BarcodeFormat, DecodeOptions};
// Реэкспорт генератора для бинарника синтетики Code128:
pub use one_d::code128::synthesize_row_code128;

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
pub fn decode_any(img: GrayImage<'_>, opts: DecodeOptions) -> Vec<Barcode> {
    let mut out = Vec::new();
    out.extend(decode_ean13_upca(&img, &opts));
    out.extend(decode_code128(&img, &opts));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Smoke-тест на синтетическом EAN-13 «5901234123457»
    #[test]
    fn smoke_synthetic_ean13_row() {
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
