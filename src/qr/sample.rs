// src/qr/sample.rs
//! Семплинг QR v1 (21×21) из изображения.
//!
//! Минимальная рабочая версия без поиска finder-паттернов:
//! предполагаем, что QR занимает центральную квадратную область кадра,
//! и равномерно дискретизируем её в сетку 21×21 ближайшим соседом.
//!
//! Когда будет подключён реальный поиск углов (finder/align), этот модуль
//! можно заменить на перспективную выборку по гомографии.

use crate::prelude::GrayImage;
use super::QrOptions;

/// Сэмплинг сетки 21×21. Возвращает `Vec<bool>` длиной 441 (true = чёрный).
///
/// Условия успеха (временно, в ожидании полноценного finder’а):
/// - картинка не пустая,
/// - минимальный размер стороны >= 21,
/// - QR приблизительно по центру и занимает существенную часть кадра.
pub fn sample_qr_v1_grid(img: &GrayImage<'_>, _opts: &QrOptions) -> Option<Vec<bool>> {
    let w = img.width as i32;
    let h = img.height as i32;
    if w <= 0 || h <= 0 {
        return None;
    }

    let side = w.min(h);
    if side < 21 {
        return None;
    }

    // Размер одного модуля в исходном изображении.
    let module = side as f32 / 21.0;

    // Координаты левой-верхней точки центрального квадрата.
    // Центрируем 21×21 квадрат внутри изображения.
    let off_x = ((w - side) as f32) * 0.5;
    let off_y = ((h - side) as f32) * 0.5;

    let mut out = Vec::with_capacity(21 * 21);

    for gy in 0..21 {
        for gx in 0..21 {
            // Берём центр модуля (gx+0.5, gy+0.5) в координатах сетки,
            // переводим в координаты исходного изображения.
            let fx = off_x + (gx as f32 + 0.5) * module;
            let fy = off_y + (gy as f32 + 0.5) * module;

            let ix = fx.round() as i32;
            let iy = fy.round() as i32;

            if ix < 0 || iy < 0 || ix >= w || iy >= h {
                return None;
            }

            // Достаём яркость из GrayImage: img.data[row_major]
            let v = img.data[(iy as usize) * (img.width) + (ix as usize)];
            let is_black = v < 128;
            out.push(is_black);
        }
    }

    Some(out)
}
