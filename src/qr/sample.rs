// src/qr/sample.rs
//! Семплинг QR v1 (21×21) из изображения c использованием найденных finder-паттернов.
//!
//! 1. Принимает 3 центра finder-паттернов.
//! 2. Упорядочивает их: BL, TL, TR (bottom-left, top-left, top-right).
//! 3. Вычисляет матрицу перспективного преобразования для отображения
//!    координат сетки 21x21 в координаты изображения.
//! 4. Семплирует сетку, используя эту матрицу.

use super::{finder::PointF, QrOptions};
use crate::prelude::GrayImage;

/// Матрица 3x3 для аффинных/перспективных преобразований.
#[derive(Debug, Clone, Copy)]
struct Transform {
    m: [f32; 9],
}

impl Transform {
    /// Создаёт матрицу перспективного преобразования, которая отображает
    /// квадрат (0,0)-(1,0)-(1,1)-(0,1) в четырёхугольник p0-p1-p2-p3.
    fn quad_to_square(p0: PointF, p1: PointF, p2: PointF, p3: PointF) -> Option<Self> {
        let dx1 = p1.x - p2.x;
        let dx2 = p3.x - p2.x;
        let dx3 = p0.x - p1.x + p2.x - p3.x;
        let dy1 = p1.y - p2.y;
        let dy2 = p3.y - p2.y;
        let dy3 = p0.y - p1.y + p2.y - p3.y;

        let det = dx1 * dy2 - dx2 * dy1;
        if det.abs() < 1e-6 {
            return None;
        }

        let a13 = (dx3 * dy2 - dx2 * dy3) / det;
        let a23 = (dx1 * dy3 - dx3 * dy1) / det;

        Some(Self {
            m: [
                p1.x - p0.x + a13 * p1.x, // a11
                p3.x - p0.x + a23 * p3.x, // a12
                p0.x,                     // a13 -> tx
                p1.y - p0.y + a13 * p1.y, // a21
                p3.y - p0.y + a23 * p3.y, // a22
                p0.y,                     // a23 -> ty
                a13,                      // a31
                a23,                      // a32
                1.0,                      // a33
            ],
        })
    }

    /// Применяет преобразование к точке (x, y).
    fn transform_point(&self, x: f32, y: f32) -> PointF {
        let den = self.m[6] * x + self.m[7] * y + self.m[8];
        PointF {
            x: (self.m[0] * x + self.m[1] * y + self.m[2]) / den,
            y: (self.m[3] * x + self.m[4] * y + self.m[5]) / den,
        }
    }
}

/// Упорядочивает три точки findera: [bottom_left, top_left, top_right].
/// TL - та, у которой два других соседа образуют примерно прямой угол.
fn order_finders(p: [PointF; 3]) -> [PointF; 3] {
    let d01 = p[0].dist2(p[1]);
    let d12 = p[1].dist2(p[2]);
    let d02 = p[0].dist2(p[2]);

    let (mut tl, mut tr, mut bl) = if d01 > d12 && d01 > d02 {
        // p2 - вершина угла
        (p[2], p[0], p[1])
    } else if d12 > d01 && d12 > d02 {
        // p0 - вершина угла
        (p[0], p[1], p[2])
    } else {
        // p1 - вершина угла
        (p[1], p[0], p[2])
    };

    // Убедимся, что TR и BL на правильных местах (используем z-компоненту векторного произведения)
    if (tr.x - tl.x) * (bl.y - tl.y) - (tr.y - tl.y) * (bl.x - tl.x) < 0.0 {
        std::mem::swap(&mut tr, &mut bl);
    }
    [bl, tl, tr]
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

    // 1. Упорядочиваем точки
    let ordered = order_finders([finders[0], finders[1], finders[2]]);
    let bl = ordered[0];
    let tl = ordered[1];
    let tr = ordered[2];

    // 2. Оцениваем 4-й угол (BR) как параллелограмм
    let br = PointF {
        x: tr.x + (bl.x - tl.x),
        y: tr.y + (bl.y - tl.y),
    };

    // 3. Вычисляем матрицу преобразования из пространства сетки в пространство изображения.
    let grid_size = 21.0;

    // Исходный квадрат в координатах сетки (центры угловых модулей)
    let src_tl = PointF { x: 3.5, y: 3.5 };
    let src_tr = PointF {
        x: grid_size - 3.5,
        y: 3.5,
    };
    let src_br = PointF {
        x: grid_size - 3.5,
        y: grid_size - 3.5,
    };
    let src_bl = PointF {
        x: 3.5,
        y: grid_size - 3.5,
    };

    // Целевой четырехугольник в координатах изображения
    let to_img_space = Transform::quad_to_square(src_tl, src_tr, src_br, src_bl)?;

    // 4. Семплируем сетку
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
            let img_p = to_img_space.transform_point(grid_p.x, grid_p.y);

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

    // 5. Бинаризация по среднему значению семплированных точек
    if grid_luma.is_empty() {
        return None;
    }
    let threshold = (sum_luma / (grid_luma.len() as u64)) as u8;

    let binarized_grid: Vec<bool> = grid_luma.into_iter().map(|luma| luma < threshold).collect();

    Some(binarized_grid)
}