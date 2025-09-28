//! Корневой модуль ультрасканера.

#![deny(rust_2018_idioms)]

pub mod core;        // тут лежат types и др.
pub mod binarize;    // нужен для qr::finder
pub mod one_d;       // (если есть в проекте; не мешает)
pub mod qr;

pub mod api;         // публичный (для tests)
pub mod compat;
pub mod prelude;

// Удобные реэкспорты наружу
pub use api::Pipeline;
pub use prelude::{
    GrayImage, LumaImage, DecodedSymbol, DecodedExtras, Symbology, Orientation, Quad,
};

// Обёртки, принимающие LumaImage и внутри конвертирующие в GrayImage.
// Они полезны, если кто-то вызывает через корень крейта.
pub fn decode_all(img: &LumaImage, pipeline: &api::Pipeline) -> Vec<DecodedSymbol> {
    pipeline.decode_all(img)
}

pub fn decode_first(img: &LumaImage, pipeline: &api::Pipeline) -> Option<DecodedSymbol> {
    pipeline.decode_first(img)
}

// Переэкспорт опций 1D-декодера и генератора синтетики Code128:
pub use crate::one_d::DecodeOptions;
pub use crate::one_d::code128::synthesize_row_code128;

// Старые bin'ы зовут ultracode::decode_any(GrayImage, DecodeOptions).
// Тонкий шлюз к текущему пайплайну.
pub fn decode_any(
    img: crate::core::types::GrayImage<'_>,
    _opts: DecodeOptions,
) -> Vec<crate::core::types::DecodedSymbol> {
    // Переливаем GrayImage (заимствованный) в LumaImage (владеющий),
    // потому что публичный decode_all сейчас принимает &LumaImage.
    let owned = crate::core::types::LumaImage {
        width: img.width,
        height: img.height,
        data: img.data.to_vec(),
    };

    let pipeline = crate::api::Pipeline::default();
    crate::decode_all(&owned, &pipeline)
}
