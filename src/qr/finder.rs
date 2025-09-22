use crate::binarize::{binarize_row_adaptive, runs};
use crate::GrayImage;
use super::QrOptions;

#[derive(Clone, Copy, Debug)]
pub struct PointF { pub x: f32, pub y: f32 }
impl PointF {
    #[inline] pub fn dist2(self, other: PointF) -> f32 {
        let dx = self.x - other.x; let dy = self.y - other.y; dx*dx + dy*dy
    }
}

/// Найти до 3-х центров finder patterns (бычьи глаза) через соотношение 1:1:3:1:1
/// по множеству горизонтальных и вертикальных сканов. Возвращает центры в пикселях.
pub fn find_finder_patterns(img: &GrayImage<'_>, opts: &QrOptions) -> Vec<PointF> {
    let mut cands: Vec<PointF> = Vec::new();

    // --- Горизонтальные сканы ---
    let rows = opts.scan_lines.max(1).min(img.height);
    for i in 0..rows {
        let y = (i * (img.height - 1)) / (rows - 1).max(1);
        let row = img.row(y);
        let rb = binarize_row_adaptive(row);
        let rl = runs(&rb);

        // Префиксные суммы для координаты x
        let mut pref = Vec::with_capacity(rl.len() + 1);
        pref.push(0usize);
        for &w in &rl { pref.push(pref.last().unwrap() + w); }

        // Цвета run'ов (true=чёрный)
        let starts_black = rb.first().copied().unwrap_or(false);
        let color_at = |idx: usize| -> bool {
            if starts_black { idx % 2 == 0 } else { idx % 2 == 1 }
        };

        for r0 in 0..rl.len().saturating_sub(4) {
            // Требуем B-W-B-W-B
            if !color_at(r0) || color_at(r0+1) || !color_at(r0+2) || color_at(r0+3) || !color_at(r0+4) {
                continue;
            }
            let win = [rl[r0], rl[r0+1], rl[r0+2], rl[r0+3], rl[r0+4]];
            if is_finder_ratio(&win) {
                let x0 = pref[r0];
                let x_center = (x0 + win[0] + win[1] + win[2]/2) as f32;
                cands.push(PointF { x: x_center, y: y as f32 });
            }
        }
    }

    // --- Вертикальные сканы ---
    let cols = opts.scan_lines.max(1).min(img.width);
    for j in 0..cols {
        let x = (j * (img.width - 1)) / (cols - 1).max(1);

        // Собираем столбец
        let mut col: Vec<u8> = Vec::with_capacity(img.height);
        for y in 0..img.height { col.push(img.get(x, y)); }
        let rb = binarize_row_adaptive(&col);
        let rl = runs(&rb);

        // Префиксы для координаты y
        let mut pref = Vec::with_capacity(rl.len() + 1);
        pref.push(0usize);
        for &w in &rl { pref.push(pref.last().unwrap() + w); }

        let starts_black = rb.first().copied().unwrap_or(false);
        let color_at = |idx: usize| -> bool {
            if starts_black { idx % 2 == 0 } else { idx % 2 == 1 }
        };

        for r0 in 0..rl.len().saturating_sub(4) {
            if !color_at(r0) || color_at(r0+1) || !color_at(r0+2) || color_at(r0+3) || !color_at(r0+4) {
                continue;
            }
            let win = [rl[r0], rl[r0+1], rl[r0+2], rl[r0+3], rl[r0+4]];
            if is_finder_ratio(&win) {
                let y0 = pref[r0];
                let y_center = (y0 + win[0] + win[1] + win[2]/2) as f32;
                cands.push(PointF { x: x as f32, y: y_center });
            }
        }
    }

    // --- Простейшая кластеризация кандидатов по расстоянию ---
    if cands.is_empty() { return Vec::new(); }

    let mut clusters: Vec<(PointF, usize)> = Vec::new(); // (center, count)
    let dist_thr = (img.width.min(img.height) as f32) / 20.0; // ~5% размера
    let dist2_thr = dist_thr * dist_thr;

    for p in cands {
        let mut assigned = false;
        for (c, cnt) in &mut clusters {
            if p.dist2(*c) <= dist2_thr {
                // инкрементальное среднее
                let k = *cnt as f32 + 1.0;
                c.x = (c.x * (*cnt as f32) + p.x) / k;
                c.y = (c.y * (*cnt as f32) + p.y) / k;
                *cnt += 1;
                assigned = true;
                break;
            }
        }
        if !assigned {
            clusters.push((p, 1));
        }
    }

    // Возвращаем 3 самых «плотных» кластера
    clusters.sort_by_key(|(_, cnt)| std::cmp::Reverse(*cnt));
    clusters.iter().take(3).map(|(c, _)| *c).collect()
}

/// Проверка окна из 5 run'ов на соотношение 1:1:3:1:1.
fn is_finder_ratio(win: &[usize;5]) -> bool {
    let sum: usize = win.iter().sum();
    if sum == 0 { return false; }
    let m = sum as f32 / 7.0; // один модуль в пикселях
    let exp = [1.0, 1.0, 3.0, 1.0, 1.0];
    let mut err = 0.0f32;
    for i in 0..5 {
        err += ((win[i] as f32) - exp[i]*m).abs() / m;
    }
    err <= 1.6 // допускаем суммарное отклонение ~1.6 модуля на окно
}

/// Синтетика: каркас QR v1 (21×21 модулей) с тремя finder и quiet zone 4 модуля.
pub fn synthesize_qr_v1_empty(unit: usize) -> GrayImage<'static> {
    synthesize_qr_internal(unit, false)
}

/// Синтетика: QR v1 skeleton — finders + timing pattern + quiet zone.
pub fn synthesize_qr_v1_skeleton(unit: usize) -> GrayImage<'static> {
    synthesize_qr_internal(unit, true)
}

fn synthesize_qr_internal(unit: usize, with_timing: bool) -> GrayImage<'static> {
    assert!(unit >= 1);
    let n = 21usize;   // версия 1
    let qz = 4usize;   // quiet zone
    let total = n + 2*qz;

    // Матрица модулей (true=чёрный, false=белый)
    let mut mods = vec![false; total * total];

    // Установить модуль
    let mut set_mod = |mx: usize, my: usize, v: bool| {
        mods[my * total + mx] = v;
    };

    // Рисуем finder 7×7 (бордер и центр 3×3 чёрные), остальное белое — даёт 1-пикс. white separator.
    fn draw_finder(set: &mut dyn FnMut(usize,usize,bool), ox: usize, oy: usize) {
        for dy in 0..7 {
            for dx in 0..7 {
                let on_border = dx == 0 || dx == 6 || dy == 0 || dy == 6;
                let in_core = (dx >= 2 && dx <= 4) && (dy >= 2 && dy <= 4);
                let v = on_border || in_core; // чёрный на бордере и в центре 3×3
                set(ox + dx, oy + dy, v);
            }
        }
    }

    let off = qz;

    // top-left
    {
        let mut s = |x,y,v| set_mod(off + x, off + y, v);
        draw_finder(&mut s, 0, 0);
    }
    // top-right
    {
        let mut s = |x,y,v| set_mod(off + (n - 7) + x, off + 0 + y, v);
        draw_finder(&mut s, 0, 0);
    }
    // bottom-left
    {
        let mut s = |x,y,v| set_mod(off + 0 + x, off + (n - 7) + y, v);
        draw_finder(&mut s, 0, 0);
    }

    // timing pattern (строка 6 и столбец 6), только в промежутках между finder'ами
    if with_timing {
        // По X: между TL и TR — колонки 8..=12 (13 — белый сепаратор TR)
        for x in 8..=12 {
            let v = (x % 2) == 0; // начинаем с чёрного на x=8
            set_mod(off + x, off + 6, v);
        }
        // По Y: между TL и BL — строки 8..=12 (13 — белый сепаратор BL)
        for y in 8..=12 {
            let v = (y % 2) == 0; // начинаем с чёрного на y=8
            set_mod(off + 6, off + y, v);
        }
    }

    // В пиксели (чёрный=0, белый=255)
    let w = total * unit;
    let h = total * unit;
    let mut data = Vec::with_capacity(w * h);
    for my in 0..total {
        for _sy in 0..unit {
            for mx in 0..total {
                let v = mods[my * total + mx];
                let px = if v { 0u8 } else { 255u8 };
                for _sx in 0..unit {
                    data.push(px);
                }
            }
        }
    }

    // Возвращаем изображение с 'static' данными (владеем буфером) — только для тестов/демо.
    let boxed = data.into_boxed_slice();
    let leaked: &'static [u8] = Box::leak(boxed);
    GrayImage { width: w, height: h, data: leaked }
}
