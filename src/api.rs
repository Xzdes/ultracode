// src/api.rs
//
// Высокоуровневый API: единая точка входа для распознавания.
// Поддержка 1D (EAN-13/UPC-A, Code128) и e2e для QR v1 (L/M/Q/H)
// с ПОЛНОЙ коррекцией RS ECC (одноблочной для v1).

use crate::one_d;
use crate::one_d::DecodeOptions;
use crate::prelude::*;

// QR-конвейер использует подмодули внутри `qr`
use crate::qr::{self, bytes, data, finder, format, rs, sample, QrOptions};

/// Опции пайплайна (задаются через Builder).
#[derive(Clone, Debug)]
pub struct PipelineOptions {
    pub enable_ean13_upca: bool,
    pub enable_code128: bool,
    pub enable_qr: bool,
    /// Разрешённые уровни коррекции ошибок для QR v1.
    /// Если пусто — считаем, что разрешены все уровни.
    pub qr_allowed_ec_levels: Vec<format::EcLevel>,
    /// Проверять RS (пересчитывать EC и сравнивать).
    pub qr_verify_rs: bool,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            enable_ean13_upca: true,
            enable_code128: true,
            enable_qr: true,
            qr_allowed_ec_levels: vec![],
            qr_verify_rs: true,
        }
    }
}

/// Builder для PipelineOptions.
#[derive(Clone, Debug)]
pub struct PipelineBuilder {
    opts: PipelineOptions,
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self {
            opts: PipelineOptions::default(),
        }
    }
}

impl PipelineBuilder {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn enable_ean13_upca(mut self, v: bool) -> Self {
        self.opts.enable_ean13_upca = v;
        self
    }

    #[inline]
    pub fn enable_code128(mut self, v: bool) -> Self {
        self.opts.enable_code128 = v;
        self
    }

    #[inline]
    pub fn enable_qr(mut self, v: bool) -> Self {
        self.opts.enable_qr = v;
        self
    }

    /// Разрешённые уровни EC для QR. Пусто => все уровни.
    #[inline]
    pub fn qr_allowed_levels(mut self, levels: &[format::EcLevel]) -> Self {
        self.opts.qr_allowed_ec_levels = levels.to_vec();
        self
    }

    /// Включить/выключить проверку RS.
    #[inline]
    pub fn qr_verify_rs(mut self, v: bool) -> Self {
        self.opts.qr_verify_rs = v;
        self
    }

    #[inline]
    pub fn build(self) -> Pipeline {
        Pipeline { opts: self.opts }
    }
}

#[derive(Clone, Debug)]
pub struct Decoder;

impl Default for Decoder {
    fn default() -> Self {
        Decoder
    }
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    opts: PipelineOptions,
}

impl Default for Pipeline {
    fn default() -> Self {
        Pipeline {
            opts: PipelineOptions::default(),
        }
    }
}

impl Pipeline {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Главная функция: распознать всё, что можем, на изображении.
    #[inline]
    pub fn decode_all(&self, img: &LumaImage) -> Vec<DecodedSymbol> {
        let mut out: Vec<DecodedSymbol> = Vec::new();

        // === 1) 1D: EAN-13 / UPC-A ===
        if self.opts.enable_ean13_upca {
            let opts = DecodeOptions::default();
            let ean = one_d::decode_ean13_upca(&img.as_gray(), &opts);
            for b in ean {
                out.push(
                    DecodedSymbol::new(Symbology::Ean13, b.text)
                        .with_confidence(0.95)
                        .with_extras(DecodedExtras::new().with("row", b.row.to_string())),
                );
            }
        }

        // === 2) 1D: Code128 ===
        if self.opts.enable_code128 {
            let opts = DecodeOptions::default();
            let c128 = one_d::decode_code128(&img.as_gray(), &opts);
            for b in c128 {
                out.push(
                    DecodedSymbol::new(Symbology::Code128, b.text)
                        .with_confidence(0.95)
                        .with_extras(DecodedExtras::new().with("row", b.row.to_string())),
                );
            }
        }

        // === 3) QR v1 (L/M/Q/H) с RS-проверкой и КОРРЕКЦИЕЙ ===
        if self.opts.enable_qr {
            if let Some(sym) = self.try_decode_qr_v1_all_levels_with_correction(img) {
                out.push(sym);
            }
        }

        // === Дедупликация по (Symbology, text) ===
        dedup_by_sym_and_text(out)
    }

    /// Сахар: вернуть первый найденный символ.
    #[inline]
    pub fn decode_first(&self, img: &LumaImage) -> Option<DecodedSymbol> {
        self.decode_all(img).into_iter().next()
    }

    /// QR v1: пробуем уровни EC (L/M/Q/H), с белым списком и КОРРЕКЦИЕЙ RS.
    fn try_decode_qr_v1_all_levels_with_correction(
        &self,
        img: &LumaImage,
    ) -> Option<DecodedSymbol> {
        let qr_opts = QrOptions::default();

        // 1. Ищем finder patterns
        let finders = finder::find_finder_patterns(&img.as_gray(), &qr_opts);
        if finders.len() < 3 {
            return None;
        }

        // 2. Семплинг сетки 21×21 (flatten: Vec<bool> длиной 441).
        let grid: Vec<bool> = sample::sample_qr_v1_grid(&img.as_gray(), &qr_opts, &finders)?;

        if grid.len() != 21 * 21 {
            return None;
        }

        // Матрица 21×21
        let mut matrix: Vec<Vec<bool>> = vec![vec![false; 21]; 21];
        for y in 0..21 {
            for x in 0..21 {
                matrix[y][x] = grid[y * 21 + x];
            }
        }

        // Формат (две копии по 15 бит) → (ec_level, mask, ...).
        let (ec_level, mask_id, _hamming_dist, _src_index) =
            qr::decode_v1_format_from_matrix(&matrix)?;

        // Белый список уровней EC (если непустой).
        if !self.opts.qr_allowed_ec_levels.is_empty()
            && !self.opts
                .qr_allowed_ec_levels
                .iter()
                .any(|&v| v == ec_level)
        {
            return None;
        }

        // Анмаск матрицы по маске mask_id и flatten → Vec<bool>.
        let unmasked = unmask_matrix_v1(&matrix, mask_id);
        let mut flat: Vec<bool> = Vec::with_capacity(21 * 21);
        for y in 0..21 {
            for x in 0..21 {
                flat.push(unmasked[y][x]);
            }
        }

        // Извлечь 208 data-бит (для v1 — фиксированная схема обхода).
        let data_bits: Vec<bool> = data::extract_data_bits_v1(&flat);

        // Разное разбиение 26 кодвордов для уровней L/M/Q/H:
        let (data_len, ec_len) = match ec_level {
            format::EcLevel::L => (19usize, 7usize),
            format::EcLevel::M => (16usize, 10usize),
            format::EcLevel::Q => (13usize, 13usize),
            format::EcLevel::H => (9usize, 17usize),
        };

        // 208 бит → 26 байт кодвордов.
        let cw_orig: Vec<u8> = bytes::bits_to_bytes_v1(&data_bits);
        if cw_orig.len() != 26 {
            return None;
        }

        // RS-проверка (по желанию) и КОРРЕКЦИЯ (всегда пробуем).
        let mut cw = cw_orig.clone();
        let mut extras = DecodedExtras::new()
            .with("qr.version", "1")
            .with("qr.ec_level", ec_level_to_str(ec_level));

        if self.opts.qr_verify_rs {
            let (d, e) = cw.split_at(data_len);
            let calc = rs::rs_ec_bytes(d, ec_len);
            let match_rs = calc == e;
            extras = extras.with("qr.rs_match", if match_rs { "true" } else { "false" });
        }

        // Пытаемся исправить ошибки *in-place*.
        let mut corrected_bytes = 0usize;
        match rs::rs_correct_codeword_block(&mut cw[..], data_len, ec_len) {
            Ok(ncorr) => {
                corrected_bytes = ncorr;
                extras = extras
                    .with("qr.rs_corrected", "true")
                    .with("qr.rs_corrected_bytes", ncorr.to_string());
            }
            Err(_) => {
                extras = extras.with("qr.rs_corrected", "false");
            }
        }

        // Парсим Byte-mode из ИСПРАВЛЕННЫХ кодвордов (если коррекция не удалась,
        // cw == cw_orig — парсим исходное).
        let bits_from_cw = bytes_to_bits_msb(&cw);
        let text: String = match bytes::parse_byte_mode_bits_v1_l(&bits_from_cw) {
            Some(t) => t,
            None => return None,
        };

        let dark_count = flat.iter().filter(|&&b| b).count();
        extras = extras.with("qr.dark_modules", dark_count.to_string());

        // Уверенность: базовая 0.80, +0.10 если RS сошёлся изначально, +0.05 если коррекция что-то исправила.
        let mut confidence = 0.80f32;
        if let Some(v) = extras.properties.get("qr.rs_match") {
            if v == "true" {
                confidence += 0.10;
            }
        }
        if corrected_bytes > 0 {
            confidence += 0.05;
        }
        if confidence > 0.99 {
            confidence = 0.99;
        }

        Some(
            DecodedSymbol::new(Symbology::QR, text)
                .with_confidence(confidence)
                .with_extras(extras),
        )
    }
}

/// Снять маску `mask_id` (0..7) — вернёт новую матрицу 21×21 с XOR маской.
fn unmask_matrix_v1(matrix: &[Vec<bool>], mask_id: u8) -> Vec<Vec<bool>> {
    let n = 21usize;
    let mut out = vec![vec![false; n]; n];
    for y in 0..n {
        for x in 0..n {
            let m = mask_predicate(mask_id, x, y);
            out[y][x] = matrix[y][x] ^ m;
        }
    }
    out
}

/// Предикаты восьми масок из ISO/IEC 18004 (0..7).
#[inline]
fn mask_predicate(mask_id: u8, x: usize, y: usize) -> bool {
    let x = x as i32;
    let y = y as i32;
    match mask_id & 7 {
        0 => ((y + x) % 2) == 0,
        1 => (y % 2) == 0,
        2 => (x % 3) == 0,
        3 => ((y + x) % 3) == 0,
        4 => (((y / 2) + (x / 3)) % 2) == 0,
        5 => (((y * x) % 2) + ((y * x) % 3)) == 0,
        6 => ((((y * x) % 2) + ((y * x) % 3)) % 2) == 0,
        7 => ((((y + x) % 2) + ((y * x) % 3)) % 2) == 0,
        _ => false,
    }
}

/// Дедупликация по (Symbology, text).
fn dedup_by_sym_and_text(mut items: Vec<DecodedSymbol>) -> Vec<DecodedSymbol> {
    use std::collections::HashSet;
    let mut seen: HashSet<(Symbology, String)> = HashSet::new();
    items.retain(|s| {
        let key = (s.symbology, s.text.clone());
        if seen.contains(&key) {
            false
        } else {
            seen.insert(key);
            true
        }
    });
    items
}

#[inline]
fn ec_level_to_str(l: format::EcLevel) -> &'static str {
    match l {
        format::EcLevel::L => "L",
        format::EcLevel::M => "M",
        format::EcLevel::Q => "Q",
        format::EcLevel::H => "H",
    }
}

/// Преобразовать байты (26 кодвордов) в 208 бит, MSB первым в каждом байте.
fn bytes_to_bits_msb(bytes: &[u8]) -> Vec<bool> {
    let mut out = Vec::with_capacity(bytes.len() * 8);
    for &b in bytes {
        for i in (0..8).rev() {
            out.push(((b >> i) & 1) != 0);
        }
    }
    out
}