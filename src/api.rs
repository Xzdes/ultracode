// src/api.rs
//
// Минималистичный верхнеуровневый API. Здесь мы пока не привязываемся
// к конкретным внутренним распознавателям — даём стабильный слой,
// который можно расширять/подменять.

use crate::prelude::*;
use crate::one_d::{self, DecodeOptions};

#[derive(Default)]
pub struct Decoder;

impl Decoder {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Default)]
pub struct Pipeline {
    _dec: Decoder,
}

impl Pipeline {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Высокоуровневая функция «распознай всё» на изображении.
    ///
    /// Реализация:
    /// - Сканируем несколько строк равномерно по высоте (как в one_d::*).
    /// - Для каждой строки пробуем EAN-13/UPC-A и Code128 слева-направо и справа-налево.
    /// - Результаты маппим в новый унифицированный DecodedSymbol.
    /// - Дубликаты (по {symbology, text, row}) отбрасываем.
    #[inline]
    pub fn decode_all(&self, img: &LumaImage) -> Vec<DecodedSymbol> {
        use std::collections::HashSet;

        let opts = DecodeOptions::default();

        let rows = opts.scan_rows.max(1).min(img.height);
        let denom = (rows - 1).max(1);

        let mut out = Vec::new();
        let mut seen = HashSet::<(Symbology, String, usize)>::new();

        for i in 0..rows {
            let y = (i * (img.height - 1)) / denom;
            let row = img.row(y);

            // --- EAN-13 / UPC-A ---
            if let Some(text) = one_d::ean13::decode_row(row, &opts) {
                let key = (Symbology::Ean13, text.clone(), y);
                if seen.insert(key) {
                    out.push(
                        DecodedSymbol::new(Symbology::Ean13, text)
                            .with_confidence(1.0)
                            .with_extras(
                                DecodedExtras::new()
                                    .with("row", y.to_string())
                                    .with("direction", "forward"),
                            ),
                    );
                }
            } else {
                let mut rev = row.to_vec();
                rev.reverse();
                if let Some(text) = one_d::ean13::decode_row(&rev, &opts) {
                    let key = (Symbology::Ean13, text.clone(), y);
                    if seen.insert(key) {
                        out.push(
                            DecodedSymbol::new(Symbology::Ean13, text)
                                .with_confidence(1.0)
                                .with_extras(
                                    DecodedExtras::new()
                                        .with("row", y.to_string())
                                        .with("direction", "reverse"),
                                ),
                        );
                    }
                }
            }

            // --- Code128 ---
            if let Some(text) = one_d::code128::decode_row(row, &opts) {
                let key = (Symbology::Code128, text.clone(), y);
                if seen.insert(key) {
                    out.push(
                        DecodedSymbol::new(Symbology::Code128, text)
                            .with_confidence(1.0)
                            .with_extras(
                                DecodedExtras::new()
                                    .with("row", y.to_string())
                                    .with("direction", "forward"),
                            ),
                    );
                }
            } else {
                let mut rev = row.to_vec();
                rev.reverse();
                if let Some(text) = one_d::code128::decode_row(&rev, &opts) {
                    let key = (Symbology::Code128, text.clone(), y);
                    if seen.insert(key) {
                        out.push(
                            DecodedSymbol::new(Symbology::Code128, text)
                                .with_confidence(1.0)
                                .with_extras(
                                    DecodedExtras::new()
                                        .with("row", y.to_string())
                                        .with("direction", "reverse"),
                                ),
                        );
                    }
                }
            }

            // --- Место под QR (когда будет готов e2e) ---
            // if let Some(sym) = qr::decode_from_image(img, y, &qr_opts) { out.push(sym); }
        }

        out
    }

    /// «Распознай первый» — удобный сахар.
    #[inline]
    pub fn decode_first(&self, img: &LumaImage) -> Option<DecodedSymbol> {
        self.decode_all(img).into_iter().next()
    }
}
