// src/compat.rs
//! Совместимость со старым API (бинарники scan_*).
//! Используем новый Pipeline и маппим результат в one_d::Barcode.

use crate::api::Pipeline;
use crate::one_d::{Barcode, BarcodeFormat, DecodeOptions};
use crate::prelude::{DecodedSymbol, LumaImage, Symbology};

/// Старый вход: LumaImage + DecodeOptions → Vec<one_d::Barcode>.
/// QR в старом enum отсутствует — такие результаты пропускаем
/// (если нужно — добавь вариант QR в BarcodeFormat и разморозь маппинг ниже).
pub fn decode_any(img: LumaImage<'_>, _opts: DecodeOptions) -> Vec<Barcode> {
    let pipeline = Pipeline::default();

    // В новом API метод принимает ссылку.
    let decoded: Vec<DecodedSymbol> = pipeline.decode_all(&img);

    let mut out = Vec::with_capacity(decoded.len());
    for s in decoded {
        let fmt = match s.symbology {
            Symbology::Code128 => Some(BarcodeFormat::Code128),
            Symbology::Ean13 => Some(BarcodeFormat::EAN13),
            Symbology::QR => None, // нет варианта в старом enum → пропускаем
        };

        if let Some(format) = fmt {
            out.push(Barcode {
                format,
                text: s.text,
                // В старой структуре `row` — это индекс строки (usize),
                // заполним 0, так как у нас нет «сырого ряда» на этом слое.
                row: 0,
            });
        }
    }
    out
}

/*
Чтобы вернуть и QR, добавь в src/one_d/mod.rs:

pub enum BarcodeFormat {
    Code128,
    EAN13,
    QR, // <— добавить
}

и здесь поменяй:
    Symbology::QR => None
на
    Symbology::QR => Some(BarcodeFormat::QR)
*/
