// src/compat.rs
//! Совместимость со старым API (бинарники scan_*).
//! Используем новый Pipeline и маппим результат в one_d::Barcode.

use crate::api::Pipeline;
use crate::one_d::{Barcode, BarcodeFormat, DecodeOptions};
use crate::prelude::{DecodedSymbol, GrayImage, LumaImage, Symbology};

/// Старый вход из бинарников: GrayImage<'_> + DecodeOptions → Vec<one_d::Barcode>.
/// Конвертируем GrayImage во «владельческий» LumaImage и запускаем новый пайплайн.
/// QR в старом enum отсутствует — пропускаем (или добавь вариант в BarcodeFormat).
pub fn decode_any(img: GrayImage<'_>, _opts: DecodeOptions) -> Vec<Barcode> {
    let pipeline = Pipeline::default();

    // Копия буфера: GrayImage<'_> → LumaImage
    let owned: LumaImage = img.into();

    // Новый API принимает &LumaImage и возвращает Vec<DecodedSymbol>.
    let decoded: Vec<DecodedSymbol> = pipeline.decode_all(&owned);

    // Маппим в старую структуру.
    let mut out = Vec::with_capacity(decoded.len());
    for s in decoded {
        let fmt_opt = match s.symbology {
            Symbology::Code128 => Some(BarcodeFormat::Code128),
            Symbology::Ean13 => Some(BarcodeFormat::EAN13),
            Symbology::QR => None, // ← если добавишь QR в BarcodeFormat — сделай Some(BarcodeFormat::QR)
        };

        if let Some(format) = fmt_opt {
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
    }
    out
}
