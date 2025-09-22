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
    pub fn new(data: &'a [u8], width: usize, height: usize) -> Self {
        debug_assert_eq!(data.len(), width * height);
        Self {
            data,
            width,
            height,
        }
    }

    /// Срез строки `y` (0..height).
    #[inline]
    pub fn row(&self, y: usize) -> &'a [u8] {
        let start = y * self.width;
        let end = start + self.width;
        &self.data[start..end]
    }

    /// Безопасная выборка пикселя (x,y).
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.data[y * self.width + x]
    }
}

/// Под совместимость/читаемость предоставляем алиас LumaImage.
pub type LumaImage<'a> = GrayImage<'a>;

/// 2D-точка (для рамок/контуров).
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    #[inline]
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Четырёхугольник (рамка найденного символа).
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Quad {
    pub p0: Point,
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
}

impl Quad {
    #[inline]
    pub fn new(p0: Point, p1: Point, p2: Point, p3: Point) -> Self {
        Self { p0, p1, p2, p3 }
    }
}

/// Ориентация/поворот символа относительно входной картинки.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Orientation {
    Upright, // 0°
    Rot90,   // 90°
    Rot180,  // 180°
    Rot270,  // 270°
    MirrorH, // зеркалирование по горизонтали
    MirrorV, // зеркалирование по вертикали
}

/// Тип распознанного символа.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Symbology {
    QR,
    Code128,
    Ean13,
    // при необходимости добавляй дальше: Aztec, PDF417, Code39, …
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

/// Унифицированный результат распознавания.
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
    pub fn with_bytes(mut self, raw: Vec<u8>) -> Self {
        self.bytes = Some(raw);
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
    /// Возвращает срез `out` длиной `width` со значениями 0/255.
    fn threshold_row_mean<'b>(&self, y: usize, window: usize, out: &'b mut Vec<u8>) -> &'b [u8];
}

impl<'a> GrayImageExt for GrayImage<'a> {
    #[inline]
    fn col<'b>(&self, x: usize, buf: &'b mut Vec<u8>) -> &'b [u8] {
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

        // Префикс-суммы для быстрого среднего по окну
        let mut ps: Vec<u32> = Vec::with_capacity(w + 1);
        ps.push(0);
        for &v in row {
            ps.push(ps.last().copied().unwrap() + v as u32);
        }

        let win = window.max(1) as isize;
        for x in 0..w {
            let l = (x as isize - win).max(0) as usize;
            let r = ((x as isize + win).min((w - 1) as isize)) as usize;
            let sum = ps[r + 1] - ps[l];
            let cnt = (r + 1 - l) as u32;
            let mean = (sum + (cnt / 2)) / cnt; // округление к ближайшему
            out[x] = if row[x] as u32 >= mean { 255 } else { 0 };
        }

        &out[..]
    }
}

/// Для удобства — альтернативное имя экстеншена.
pub use GrayImageExt as LumaImageExt;
