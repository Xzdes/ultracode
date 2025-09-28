//! Часто используемые типы, реэкспортированные для удобства.

pub use crate::core::types::{
    // изображения
    GrayImage,    // буфер оттенков серого, который ждёт пайплайн
    LumaImage,    // внешний тип, из которого делаем .as_gray()

    // результаты декодирования
    DecodedSymbol,
    DecodedExtras,

    // прочее
    Symbology,
    Orientation,
    Quad,
};
