use super::{find_finder_patterns, PointF, QrOptions};
use crate::GrayImage;

/// Сэмплирует сетку QR v1 (21×21) → Vec<bool> длиной 441 (row-major, true=чёрный).
pub fn sample_qr_v1_grid(img: &GrayImage<'_>, opts: &QrOptions) -> Option<Vec<bool>> {
    let pts = find_finder_patterns(img, opts);
    if pts.len() != 3 { return None; }
    let (tl, tr, bl) = classify_tl_tr_bl(&pts)?;

    // Восстанавливаем четвёртую вершину (параллелограмм)
    let br = PointF { x: tr.x + bl.x - tl.x, y: tr.y + bl.y - tl.y };

    // Источники (в модульных координатах): центры finder'ов на 3.5 от края области данных.
    let n = 21.0f32;
    let s_tl = PointF { x: 3.5, y: 3.5 };
    let s_tr = PointF { x: n - 3.5, y: 3.5 };
    let s_bl = PointF { x: 3.5, y: n - 3.5 };
    let s_br = PointF { x: n - 3.5, y: n - 3.5 };

    let h = homography_from_4([s_tl, s_tr, s_bl, s_br], [tl, tr, bl, br])?;

    // Сэмплируем центры модулей (x+0.5, y+0.5), x,y=0..20
    let mut out = vec![false; 21*21];
    for y in 0..21 {
        for x in 0..21 {
            let sx = x as f32 + 0.5;
            let sy = y as f32 + 0.5;
            let (px, py) = project(&h, sx, sy);
            let ix = px.round() as isize;
            let iy = py.round() as isize;
            let v = if ix >= 0 && iy >= 0 && (ix as usize) < img.width && (iy as usize) < img.height {
                img.get(ix as usize, iy as usize) < 128
            } else {
                false
            };
            out[y*21 + x] = v;
        }
    }
    Some(out)
}

/// Классифицировать три finder-центра в (TL, TR, BL).
/// Находим вершину с близким к 90° углом (минимальный |dot|) → это TL.
/// Остальные две разделяем по Y: меньшая Y → TR (в экранных координатах Y вниз).
fn classify_tl_tr_bl(pts: &[PointF]) -> Option<(PointF, PointF, PointF)> {
    if pts.len() != 3 { return None; }
    let p0 = pts[0]; let p1 = pts[1]; let p2 = pts[2];

    // Посчитаем |dot| в каждой вершине
    let (tl, a, b) = {
        let dot0 = dot(sub(p1,p0), sub(p2,p0)).abs();
        let dot1 = dot(sub(p0,p1), sub(p2,p1)).abs();
        let dot2 = dot(sub(p0,p2), sub(p1,p2)).abs();
        if dot0 <= dot1 && dot0 <= dot2 { (p0, p1, p2) }
        else if dot1 <= dot0 && dot1 <= dot2 { (p1, p0, p2) }
        else { (p2, p0, p1) }
    };

    // TR — с меньшей Y, BL — с большей Y (в обычной ориентации).
    let (tr, bl) = if a.y <= b.y { (a, b) } else { (b, a) };
    Some((tl, tr, bl))
}

#[inline] fn sub(a: PointF, b: PointF) -> PointF { PointF{ x: a.x - b.x, y: a.y - b.y } }
#[inline] fn dot(a: PointF, b: PointF) -> f32 { a.x*b.x + a.y*b.y }

/// 3×3 гомография h, такая что [X,Y,1]^T ~ h * [x,y,1]^T (источник → приемник).
fn homography_from_4(src: [PointF;4], dst: [PointF;4]) -> Option<[[f32;3];3]> {
    // Решаем a*h_vec = b для h_vec=[h0..h7], h8=1
    let mut a = [[0f32;8];8];
    let mut bvec = [0f32;8];
    for k in 0..4 {
        let (x, y) = (src[k].x, src[k].y);
        let (xd, yd) = (dst[k].x, dst[k].y);
        // строка для X
        a[2*k][0] = x; a[2*k][1] = y; a[2*k][2] = 1.0;
        a[2*k][3] = 0.0; a[2*k][4] = 0.0; a[2*k][5] = 0.0;
        a[2*k][6] = -x*xd; a[2*k][7] = -y*xd;
        bvec[2*k] = xd;
        // строка для Y
        a[2*k+1][0] = 0.0; a[2*k+1][1] = 0.0; a[2*k+1][2] = 0.0;
        a[2*k+1][3] = x; a[2*k+1][4] = y; a[2*k+1][5] = 1.0;
        a[2*k+1][6] = -x*yd; a[2*k+1][7] = -y*yd;
        bvec[2*k+1] = yd;
    }
    let h_vec = solve_8x8(a, bvec)?;
    let h = [
        [h_vec[0], h_vec[1], h_vec[2]],
        [h_vec[3], h_vec[4], h_vec[5]],
        [h_vec[6], h_vec[7], 1.0    ],
    ];
    Some(h)
}

#[inline]
fn project(h: &[[f32;3];3], x: f32, y: f32) -> (f32, f32) {
    let nx = h[0][0]*x + h[0][1]*y + h[0][2];
    let ny = h[1][0]*x + h[1][1]*y + h[1][2];
    let d  = h[2][0]*x + h[2][1]*y + h[2][2];
    if d.abs() < 1e-6 { return (nx, ny); }
    (nx/d, ny/d)
}

/// Наивный Гаусс для 8×8 (float). Достаточно для наших тестов.
fn solve_8x8(mut a: [[f32;8];8], mut b: [f32;8]) -> Option<[f32;8]> {
    // Прямой ход
    for i in 0..8 {
        // поиск пивота
        let mut piv = i;
        let mut mx = a[i][i].abs();
        for r in (i+1)..8 {
            let v = a[r][i].abs();
            if v > mx { mx = v; piv = r; }
        }
        if mx < 1e-8 { return None; }
        if piv != i {
            a.swap(i, piv);
            b.swap(i, piv);
        }
        // нормируем строку
        let diag = a[i][i];
        for j in i..8 { a[i][j] /= diag; }
        b[i] /= diag;

        // зануляем ниже
        for r in (i+1)..8 {
            let factor = a[r][i];
            if factor == 0.0 { continue; }
            for j in i..8 { a[r][j] -= factor * a[i][j]; }
            b[r] -= factor * b[i];
        }
    }
    // Обратный ход
    let mut x = [0f32;8];
    for i in (0..8).rev() {
        let mut s = b[i];
        for j in (i+1)..8 { s -= a[i][j] * x[j]; }
        x[i] = s; // a[i][i] уже 1
    }
    Some(x)
}
