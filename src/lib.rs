#![forbid(unsafe_code)]
#![deny(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

// Публичные модули
pub mod api;      // высокий уровень: пайплайн, трейт Decoder
pub mod core;     // общие типы/утилиты (GrayImage и др.)
pub mod prelude;  // удобные re-export'ы

pub mod one_d;    // 1D декодеры (ean13, code128)
pub mod qr;       // утилиты QR (format и пр.)
pub mod binarize; // быстрая бинаризация для 1D

// Реэкспорт базового типа изображения в корень
pub use crate::core::types::GrayImage;

// Слой совместимости со старым API (decode_any и пр.)
mod compat;
pub use compat::*;

// === ВАЖНО: реэкспорты для бинарников scan_* ===
// Раньше они писали `use ultracode::{decode_any, DecodeOptions, GrayImage};` и т.п.
// Чтобы ничего в них не менять — реэкспортируем здесь.
pub use crate::one_d::DecodeOptions;
pub use crate::one_d::{Barcode, BarcodeFormat};

// Нужен также синтезатор для демо Code128:
pub use crate::one_d::code128::synthesize_row_code128;

// Быстрый «сахар»: функции, принимающие Pipeline и LumaImage.
// (Сейчас Pipeline пустой — добавляй декодеры внутри Pipeline::decode_all)
use crate::api::Pipeline;
use crate::core::types::{DecodedSymbol, LumaImage};

/// Универсальный one-shot: прогоняет изображение через зарегистрированные декодеры.
/// По умолчанию пайплайн пустой (ты добавляешь декодеры сам через Pipeline::add).
#[inline]
pub fn decode_all(img: &LumaImage, pipeline: &Pipeline) -> Vec<DecodedSymbol> {
    pipeline.decode_all(img)
}

/// Упроститель: вернуть первый найденный символ (если важна скорость TTF).
#[inline]
pub fn decode_first(img: &LumaImage, pipeline: &Pipeline) -> Option<DecodedSymbol> {
    pipeline.decode_first(img)
}
