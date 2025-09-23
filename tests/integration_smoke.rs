// tests/integration_smoke.rs
//
// Интеграционные «дымовые» тесты верхнего уровня.
// Здесь фиксируем, что Pipeline/Builder компилируются и базовые пути работают.
// Тесты с реальными изображениями помечены #[ignore] — снимем, когда положим файлы.

use ultracode::prelude::*;
use ultracode::api::{Pipeline, PipelineBuilder};
use ultracode::one_d::DecodeOptions;

#[test]
fn pipeline_builder_compiles_and_runs_minimal() {
    // Пустая картинка 32x32 — ничего не найдём, но пайплайн должен отработать.
    let img = LumaImage { data: vec![255; 32*32], width: 32, height: 32 };

    let pipe = PipelineBuilder::new()
        .enable_ean13_upca(true)
        .enable_code128(true)
        .enable_qr(true)
        .build();

    let res = pipe.decode_all(&img);
    assert!(res.is_empty());
}

#[test]
fn pipeline_disable_qr_still_allows_1d() {
    // Синтетика для 1D тут не генерируется, этот тест — просто smoke.
    let img = LumaImage { data: vec![0; 64*64], width: 64, height: 64 };

    let pipe = PipelineBuilder::new()
        .enable_qr(false)
        .build();

    let _ = pipe.decode_all(&img);
}

/// Когда добавим реальные картинки (например, assets/qr_v1_l.png),
/// снимем ignore и проверим фактическую строку.
#[ignore]
#[test]
fn decode_real_qr_v1_l_from_png() {
    // TODO: загрузить PNG → grayscale → LumaImage → Pipeline::decode_all
    // assert!(contains expected text)
}

/// Аналогично — для других уровней EC (M/Q/H).
#[ignore]
#[test]
fn decode_real_qr_v1_m_from_png() {
    // TODO
}
