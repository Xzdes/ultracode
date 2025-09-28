use crate::api::Pipeline;
use crate::prelude::*;

/// Совместимая обёртка: принимает LumaImage и запускает пайплайн по умолчанию.
pub fn decode(img: &LumaImage) -> Vec<DecodedSymbol> {
    let pipeline = Pipeline::default();
    pipeline.decode_all(img)
}

/// То же самое, но возвращает только первый результат.
pub fn decode_first(img: &LumaImage) -> Option<DecodedSymbol> {
    let pipeline = Pipeline::default();
    pipeline.decode_first(img)
}

/// Вариант с уже сконфигурированным пайплайном.
pub fn decode_with_pipeline(img: &LumaImage, pipeline: &Pipeline) -> Vec<DecodedSymbol> {
    pipeline.decode_all(img)
}
