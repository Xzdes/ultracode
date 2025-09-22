// src/api.rs
//
// Минималистичный верхнеуровневый API. Здесь мы пока не привязываемся
// к конкретным внутренним распознавателям — даём стабильный слой,
// который можно расширять/подменять.

use crate::prelude::*;

#[derive(Default)]
pub struct Decoder;

impl Decoder {
    #[inline]
    pub fn new() -> Self { Self::default() }
}

#[derive(Default)]
pub struct Pipeline {
    dec: Decoder,
}

impl Pipeline {
    #[inline]
    pub fn new() -> Self { Self::default() }

    /// Высокоуровневая функция «распознай всё» на изображении.
    /// Сейчас — заглушка; вернёт пустой список, если ты не подключишь
    /// конкретные распознаватели здесь (QR/Code128/EAN13).
    #[inline]
    pub fn decode_all(&self, _img: &LumaImage) -> Vec<DecodedSymbol> {
        // TODO: сюда можно добавить вызовы реальных декодеров.
        // Например:
        // let mut out = Vec::new();
        // out.extend(qr::try_decode_all(_img)?);
        // out.extend(code128::try_decode_all(_img)?);
        // out
        Vec::new()
    }

    /// «Распознай первый» — удобный сахар.
    #[inline]
    pub fn decode_first(&self, img: &LumaImage) -> Option<DecodedSymbol> {
        self.decode_all(img).into_iter().next()
    }
}
