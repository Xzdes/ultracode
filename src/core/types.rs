// src/core/types.rs
//
// Общие типы и утилиты, независимые от конкретных декодеров.

use std::collections::BTreeMap;

/// Простое представление градаций серого.
/// Буфер `data` — построчно, по строкам (row-major), 8 бит на пиксель.
#[derive(Clone, Copy, Debug)]
pub struct GrayImage<'a> {
    pub data: &'a [u8],
    pub width: usize,
    pub height: usize,
}

impl<'a> GrayImage<'a> {
    #[inline]
    pub fn row(&self, y: usize) -> &'a [u8] {
        let start = y * self.width;
        &self.data[start..start + self.width]
    }

    #[inline]
    pub fn col<'b>(&self, x: usize, buf: &'b mut Vec<u8>) -> &'b [u8] {
        buf.clear();
        buf.reserve(self.height);
        for y in 0..self.height {
            buf.push(self.data[y * self.width + x]);
        }
        &buf[..]
    }

    fn threshold_row_mean<'b>(&self, y: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8] {
        let row = self.row(y);
        let w = self.width;

        out.clear();
        out.resize(w, 0);

        if w == 0 {
            return &out[..];
        }

        // простая скользящая
        let win = window.max(3).min(63) | 1;
        let r = win / 2;

        let mut sum: u32 = 0;
        for i in 0..win.min(w) {
            sum += row[i] as u32;
        }

        for x in 0..w {
            let l = x.saturating_sub(r);
            let rr = (x + r + 1).min(w);
            if rr > win {
                // скользящее окно
                let outv = row[l.saturating_sub(1)] as i32;
                let inv = row[rr - 1] as i32;
                sum = (sum as i32 - outv + inv) as u32;
            }
            let avg = (sum / (rr - l) as u32) as u8;
            out[x] = if row[x] > avg { 255 } else { 0 };
        }

        &out[..]
    }

    fn threshold_col_mean<'b>(&self, x: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8] {
        let mut col = Vec::with_capacity(self.height);

        let w = self.width;
        let h = self.height;

        out.clear();
        out.resize(h, 0);

        if h == 0 {
            return &out[..];
        }

        for y in 0..h {
            col.push(self.data[y * w + x]);
        }

        let win = window.max(3).min(63) | 1;
        let r = win / 2;

        let mut sum: u32 = 0;
        for i in 0..win.min(h) {
            sum += col[i] as u32;
        }

        for y in 0..h {
            let t = y.saturating_sub(r);
            let b = (y + r + 1).min(h);
            if b > win {
                let outv = col[t.saturating_sub(1)] as i32;
                let inv = col[b - 1] as i32;
                sum = (sum as i32 - outv + inv) as u32;
            }
            let avg = (sum / (b - t) as u32) as u8;
            out[y] = if col[y] > avg { 255 } else { 0 };
        }

        &out[..]
    }
}

/// LumaImage — «владельческая» картинка, удобная для пайплайна.
#[derive(Clone, Debug)]
pub struct LumaImage {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

impl LumaImage {
    #[inline]
    pub fn as_gray(&self) -> GrayImage<'_> {
        GrayImage {
            data: &self.data,
            width: self.width,
            height: self.height,
        }
    }

    #[inline]
    pub fn row(&self, y: usize) -> &[u8] {
        let start = y * self.width;
        &self.data[start..start + self.width]
    }

    #[inline]
    pub fn col<'b>(&self, x: usize, buf: &'b mut Vec<u8>) -> &'b [u8] {
        buf.clear();
        buf.reserve(self.height);
        for y in 0..self.height {
            buf.push(self.data[y * self.width + x]);
        }
        &buf[..]
    }

    #[inline]
    pub fn threshold_row_mean<'b>(&self, y: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8] {
        self.as_gray().threshold_row_mean(y, window, out)
    }

    #[inline]
    pub fn threshold_col_mean<'b>(&self, x: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8] {
        self.as_gray().threshold_col_mean(x, window, out)
    }
}

/// Позволяем делать `.into()` из GrayImage в LumaImage (копия буфера).
impl<'a> From<GrayImage<'a>> for LumaImage {
    #[inline]
    fn from(g: GrayImage<'a>) -> Self {
        Self {
            data: g.data.to_vec(),
            width: g.width,
            height: g.height,
        }
    }
}

/// Геометрия/вспомогательные типы.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Quad {
    pub p0: Point,
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Orientation {
    Rot0,
    Rot90,
    Rot180,
    Rot270,
    MirrorH,
    MirrorV,
}

/// Тип распознанного символа.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Symbology {
    QR,
    Code128,
    Ean13,
}

/// Ошибки распознавания верхнего уровня.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    /// На изображении не найдено ни одного поддерживаемого кода.
    NotFound,
    /// Ошибка контрольной суммы.
    ChecksumError,
    /// Формат найден, но структура неверна/повреждена.
    InvalidFormat,
    /// Внутренняя ошибка декодера/параметров (зарезервировано).
    Internal(String),
}

/// Дополнительная произвольная мета-информация о распознавании.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecodedExtras {
    pub properties: BTreeMap<String, String>,
}

impl DecodedExtras {
    #[inline]
    pub fn new() -> Self {
        Self {
            properties: BTreeMap::new(),
        }
    }

    #[inline]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedSymbol {
    pub symbology: Symbology,
    pub text: String,
    pub confidence: f32, // 0..=1
    pub quad: Option<Quad>,
    pub orientation: Option<Orientation>,
    pub bytes: Option<Vec<u8>>,
    pub extras: DecodedExtras,
}

impl DecodedSymbol {
    #[inline]
    pub fn new(symbology: Symbology, text: impl Into<String>) -> Self {
        Self {
            symbology,
            text: text.into(),
            confidence: 1.0,
            quad: None,
            orientation: None,
            bytes: None,
            extras: DecodedExtras::new(),
        }
    }

    #[inline]
    pub fn with_confidence(mut self, c: f32) -> Self {
        self.confidence = c;
        self
    }
    #[inline]
    pub fn with_quad(mut self, q: Quad) -> Self {
        self.quad = Some(q);
        self
    }
    #[inline]
    pub fn with_orientation(mut self, o: Orientation) -> Self {
        self.orientation = Some(o);
        self
    }
    #[inline]
    pub fn with_bytes(mut self, b: Vec<u8>) -> Self {
        self.bytes = Some(b);
        self
    }
    #[inline]
    pub fn with_extras(mut self, extras: DecodedExtras) -> Self {
        self.extras = extras;
        self
    }
}

/// Утилиты для GrayImage с корректными lifetime.
pub trait GrayImageExt {
    /// Вернуть столбец `x` в буфер `buf` и отдать срез с тем же lifetime.
    fn col<'b>(&self, x: usize, buf: &'b mut Vec<u8>) -> &'b [u8];

    /// Бинаризация строки `y` по скользящему среднему в окне `window`.
    fn threshold_row_mean<'b>(&self, y: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8];

    /// Бинаризация столбца `x` по скользящему среднему в окне `window`.
    fn threshold_col_mean<'b>(&self, x: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8];
}

impl<'a> GrayImageExt for GrayImage<'a> {
    #[inline]
    fn col<'b>(&self, x: usize, buf: &'b mut Vec<u8>) -> &'b [u8] {
        self.col(x, buf)
    }

    #[inline]
    fn threshold_row_mean<'b>(&self, y: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8] {
        self.threshold_row_mean(y, window, out)
    }

    #[inline]
    fn threshold_col_mean<'b>(&self, x: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8] {
        self.threshold_col_mean(x, window, out)
    }
}

impl LumaImageExt for LumaImage {
    #[inline]
    fn col<'b>(&self, x: usize, buf: &'b mut Vec<u8>) -> &'b [u8] {
        self.col(x, buf)
    }

    #[inline]
    fn threshold_row_mean<'b>(&self, y: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8] {
        self.threshold_row_mean(y, window, out)
    }

    #[inline]
    fn threshold_col_mean<'b>(&self, x: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8] {
        self.threshold_col_mean(x, window, out)
    }
}

/// Простой prelude-трейты, чтобы удобно импортировать.
pub trait LumaImageExt {
    fn col<'b>(&self, x: usize, buf: &'b mut Vec<u8>) -> &'b [u8];
    fn threshold_row_mean<'b>(&self, y: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8];
    fn threshold_col_mean<'b>(&self, x: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8];
}
