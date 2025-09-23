// src/compat.rs
//! Совместимость со старым API (бинарники scan_*).
//! Используем новый Pipeline и маппим результат в one_d::Barcode.

use crate::api::Pipeline;
use crate::one_d::{Barcode, BarcodeFormat, DecodeOptions};
use crate::prelude::{DecodedSymbol, GrayImage, LumaImage, Symbology};

/// Старый вход из бинарников: GrayImage<'_> + DecodeOptions → Vec<one_d::Barcode>.
/// Конвертируем GrayImage во «владельческий» LumaImage и запускаем новый пайплайн.
/// Теперь поддерживаем и QR через добавленный вариант BarcodeFormat::QR.
pub fn decode_any(img: GrayImage<'_>, _opts: DecodeOptions) -> Vec<Barcode> {
    let pipeline = Pipeline::default();

    let owned: LumaImage = img.into();

    let decoded: Vec<DecodedSymbol> = pipeline.decode_all(&owned);

    let mut out = Vec::with_capacity(decoded.len());
    for s in decoded {
        let format = match s.symbology {
            Symbology::Code128 => BarcodeFormat::Code128,
            Symbology::Ean13 => BarcodeFormat::EAN13,
            Symbology::QR => BarcodeFormat::QR,
        };

        // Попробуем вытащить y-координату строки, если она была положена в extras (для 1D).
        let row = s
            .extras
            .properties
            .get("row")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);

        out.push(Barcode {
            format,
            text: s.text,
            row,
        });
    }
    out
}
