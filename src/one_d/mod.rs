pub mod ean13;

use crate::GrayImage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarcodeFormat {
    EAN13,
    UPCA,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Barcode {
    pub format: BarcodeFormat,
    pub text: String,
    /// y-координата строки (для 1D сканирования).
    pub row: usize,
}

#[derive(Clone, Debug)]
pub struct DecodeOptions {
    /// Сколько строк сканировать (равномерно по высоте).
    pub scan_rows: usize,
    /// Игнорировать слабые/короткие кандидаты.
    pub min_modules: usize,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            scan_rows: 15,
            min_modules: 30,
        }
    }
}

/// Декодировать EAN-13/UPC-A сканированием нескольких строк.
pub fn decode_ean13_upca(img: &GrayImage<'_>, opts: &DecodeOptions) -> Vec<Barcode> {
    let mut out = Vec::new();
    let rows = opts.scan_rows.max(1).min(img.height);
    for i in 0..rows {
        // равномерная выборка строк по высоте
        let y = (i * (img.height - 1)) / (rows - 1).max(1);
        let row = img.row(y);
        if let Some(text) = ean13::decode_row(row, opts) {
            let (format, normalized) = if text.len() == 12 {
                (BarcodeFormat::UPCA, text)
            } else {
                (BarcodeFormat::EAN13, text)
            };
            out.push(Barcode { format, text: normalized, row: y });
        }
    }
    out
}
