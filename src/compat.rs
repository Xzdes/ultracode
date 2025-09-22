//! Совместимый слой для старого root-API:
//! - decode_any(img, opts) -> Vec<Barcode>
//! - DecodeOptions (реэкспорт из one_d)
//! - synthesize_row_code128(...)
//!
//! Нужен только для сборки бинарников из src/bin/*.
//! В прод-коде рекомендую пользоваться Pipeline/Decoder из api.rs.

use crate::core::types::GrayImage;

// Ранее эти типы были в корне; сейчас — внутри one_d. Делаем явные реэкспорты.
pub use crate::one_d::{Barcode, BarcodeFormat, DecodeOptions};

/// Простейшая (и очень быстрая) заглушка для старой функции `decode_any`.
/// Возвращает пустой вектор. Бинарники из `src/bin/*` компилируются и запускаются,
/// а поиск штрих-кодов делай через новый Pipeline (api::Pipeline).
///
/// Если хочешь реальную работу как раньше — раскомментируй блок с Pipeline ниже.
pub fn decode_any(_img: GrayImage<'_>, _opts: DecodeOptions) -> Vec<Barcode> {
    // ---- Вариант с реальным пайплайном (если уже используешь его в проекте) ----
    // use crate::api::Pipeline;
    // let mut pipeline = Pipeline::default();
    // let decoded = pipeline.decode_all(_img);
    // decoded
    //     .into_iter()
    //     .map(|s| Barcode {
    //         format: match s.symbology {
    //             crate::prelude::Symbology::QR => BarcodeFormat::QR,
    //             crate::prelude::Symbology::Code128 => BarcodeFormat::Code128,
    //             crate::prelude::Symbology::Ean13 => BarcodeFormat::Ean13,
    //         },
    //         text: s.text,
    //     })
    //     .collect()

    // Пока оставим «заглушкой», чтобы не менять логику твоего пайплайна.
    Vec::new()
}

/// Синтетический генератор «похожей» на Code128 строки пикселей (1×W),
/// чтобы бинарник `scan_code128_synth.rs` успешно собирался и мог
/// показать «полоски». Это *не* строгий кодировщик Code128.
/// Он годится для отрисовки демо-полос, а не для печати настоящих штрих-кодов.
pub fn synthesize_row_code128(text: &str, _set: char, unit: usize) -> Vec<u8> {
    // ширина: тихая зона + «полосы» по символам + стоп + тихая зона
    // для наглядности берём ~11 модулей на символ, как у Code128-паттернов
    let quiet = 10 * unit;
    let per_char = 11 * unit;
    let stop = 13 * unit;

    let width = quiet + text.len() * per_char + stop + quiet;
    let mut row = vec![0u8; width];

    // Нарисуем чёрно-белые полосы с шагом unit, меняя фазу по символам,
    // чтобы результат хоть как-то напоминал штрих-код.
    let mut on = true;
    let mut x = quiet;

    for (idx, ch) in text.chars().enumerate() {
        // простая «энтропия» от символа, чтобы ширины немного варьировались
        let seed = (ch as u32 as usize).wrapping_mul(1315423911 ^ (idx * 97));
        let mut w_left = per_char;
        let mut step = 1;

        while w_left > 0 {
            let w = ((seed >> (step & 7)) & 3) + 1; // 1..=4 юнита
            let w = w.min(w_left);
            if on {
                for i in 0..w {
                    row[x + i] = 255;
                }
            }
            x += w;
            w_left -= w;
            on = !on;
            step += 1;
        }
        // слегка меняем фазу для следующего символа
        on = ((seed ^ idx) & 1) == 0;
    }

    // «стоп»-полоса — просто несколько чёрных сегментов
    let mut w_left = stop;
    while w_left > 0 {
        let w = unit.min(w_left);
        for i in 0..w {
            row[x + i] = 255;
        }
        x += w;
        w_left -= w;
        // белый зазор
        let gap = (unit / 2).max(1);
        x += gap.min(width.saturating_sub(x));
    }

    // правый «тихий» край уже нули
    row
}
