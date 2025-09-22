#![forbid(unsafe_code)]
#![deny(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod core;    // новые общие типы/утилиты
pub mod api;     // высокий уровень: пайплайн, трейт Decoder
pub mod prelude; // удобные re-export'ы

pub mod one_d;   // ваши существующие 1D декодеры (ean13, code128)
pub mod qr;      // утилиты QR (format и пр.)

pub mod binarize;                 // <— новый модуль с бинаризацией
pub use crate::core::types::GrayImage; // <— реэкспорт типа в корень

mod compat;
pub use compat::*;


// Быстрый «сахар»: сразу готовая фабрика пайплайна со стандартным набором
// (пока без жёстких зависимостей от конкретных модулей).
use crate::api::{Decoder, Pipeline};
use crate::core::types::{DecodedSymbol, LumaImage};

/// Универсальный one-shot: прогоняет изображение через зарегистрированные декодеры.
/// По умолчанию пайплайн пустой (ты добавляешь декодеры сам через Pipeline::add).
pub fn decode_all(img: &LumaImage, pipeline: &Pipeline) -> Vec<DecodedSymbol> {
    pipeline.decode_all(img)
}

/// Упроститель: вернуть первый найденный символ (если важна скорость TTF).
pub fn decode_first(img: &LumaImage, pipeline: &Pipeline) -> Option<DecodedSymbol> {
    pipeline.decode_first(img)
}
