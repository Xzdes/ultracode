// src/qr/sample.rs
//! Семплинг QR v1 (21×21) из изображения c использованием найденных finder-паттернов.
//!
//! 1. Принимает 3 центра finder-паттернов.
//! 2. Упорядочивает их: BL, TL, TR (bottom-left, top-left, top-right).
//! 3. Вычисляет матрицу аффинного преобразования для отображения
//!    координат сетки 21x21 в координаты изображения.
//! 4. Семплирует сетку, используя эту матрицу.

use super::{finder::{self, PointF}, QrOptions};
use crate::prelude::GrayImage;

/// Аффинное преобразование: отображает координаты сетки (grid) в координаты изображения (image).
#[derive(Debug, Clone, Copy)]
struct AffineTransform {
    a: f32, b: f32, c: f32, // x_img = a*x_grid + b*y_grid + c
    d: f32, e: f32, f: f32, // y_img = d*x_grid + e*y_grid + f
}

impl AffineTransform {
    /// Создаёт преобразование на основе трёх пар соответствующих точек.
    /// `src` - точки в исходной системе координат (идеальная сетка QR).
    /// `dst` - точки в целевой системе координат (изображение).
    fn from_points(dst_tl: PointF, dst_tr: PointF, dst_bl: PointF) -> Self {
        // Координаты центров finder'ов в идеальной сетке v1 (21x21).
        // Центр модуля (0,0) имеет координаты (0.5, 0.5).
        // Центры finder'ов находятся в модулях 3,3; 17,3; 3,17.
        let src_tl_x = 3.5;
        let src_tl_y = 3.5;
        let src_tr_x = 17.5;
        let _src_tr_y = 3.5; // unused but kept for clarity
        let _src_bl_x = 3.5; // unused but kept for clarity
        let src_bl_y = 17.5;

        // Решаем систему линейных уравнений 2x3:
        // dst.x = a*src.x + b*src.y + c
        // dst.y = d*src.y + e*src.y + f
        
        let a = (dst_tr.x - dst_tl.x) / (src_tr_x - src_tl_x);
        let b = (dst_bl.x - dst_tl.x) / (src_bl_y - src_tl_y);
        let c = dst_tl.x - a * src_tl_x - b * src_tl_y;

        let d = (dst_tr.y - dst_tl.y) / (src_tr_x - src_tl_x);
        let e = (dst_bl.y - dst_tl.y) / (src_bl_y - src_tl_y);
        let f = dst_tl.y - d * src_tl_x - e * src_tl_y;

        Self { a, b, c, d, e, f }
    }

    /// Применяет преобразование к точке из пространства сетки.
    fn transform_point(&self, grid_x: f32, grid_y: f32) -> PointF {
        let img_x = self.a * grid_x + self.b * grid_y + self.c;
        let img_y = self.d * grid_x + self.e * grid_y + self.f;
        PointF { x: img_x, y: img_y }
    }
}

/// Сэмплинг сетки 21×21. Возвращает `Vec<bool>` длиной 441 (true = чёрный).
/// Принимает три найденных центра finder-паттернов.
pub fn sample_qr_v1_grid(
    img: &GrayImage<'_>,
    _opts: &QrOptions,
    finders: &[PointF],
) -> Option<Vec<bool>> {
    if finders.len() < 3 {
        return None;
    }

    // 1. Упорядочиваем точки: [bottom_left, top_left, top_right]
    let ordered = finder::order_finders([finders[0], finders[1], finders[2]]);
    let bl = ordered[0];
    let tl = ordered[1];
    let tr = ordered[2];

    // 2. Вычисляем аффинное преобразование из пространства сетки в пространство изображения.
    let transform = AffineTransform::from_points(tl, tr, bl);
    
    // 3. Семплируем сетку
    let mut grid_luma = Vec::with_capacity(21 * 21);
    let mut sum_luma = 0u64;

    for gy in 0..21 {
        for gx in 0..21 {
            // Центр модуля в координатах сетки
            let grid_p = PointF {
                x: gx as f32 + 0.5,
                y: gy as f32 + 0.5,
            };
            // Отображаем в координаты изображения
            let img_p = transform.transform_point(grid_p.x, grid_p.y);

            let ix = img_p.x.round() as usize;
            let iy = img_p.y.round() as usize;

            if ix >= img.width || iy >= img.height {
                return None; // Точка вышла за пределы изображения
            }

            let luma = img.data[iy * img.width + ix];
            grid_luma.push(luma); // Сначала сохраняем яркость
            sum_luma += u64::from(luma);
        }
    }

    // 4. Бинаризация по среднему значению семплированных точек
    if grid_luma.is_empty() {
        return None;
    }
    let threshold = (sum_luma / (grid_luma.len() as u64)) as u8;

    let binarized_grid: Vec<bool> = grid_luma.into_iter().map(|luma| luma < threshold).collect();

    Some(binarized_grid)
}