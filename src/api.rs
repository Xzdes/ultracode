// src/api.rs
//
// Высокоуровневый API: единая точка входа для распознавания.
// Поддержка 1D (EAN-13/UPC-A, Code128) и QR v1 (L/M/Q/H) с проверкой/коррекцией RS.

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
    /// Проверять и логировать совпадение RS перед коррекцией.
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
        Self { opts: PipelineOptions::default() }
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

/// Конвейер распознавания.
#[derive(Clone, Debug)]
pub struct Pipeline {
    opts: PipelineOptions,
}

impl Default for Pipeline {
    fn default() -> Self {
        Pipeline { opts: PipelineOptions::default() }
    }
}

impl Pipeline {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Совместимость с существующим вызовом из `lib.rs`: вернуть первый найденный символ.
    #[inline]
    pub fn decode_first(&self, img: &LumaImage) -> Option<DecodedSymbol> {
        self.decode_all(img).into_iter().next()
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

        // === 3) QR v1 (L/M/Q/H) ===
        if self.opts.enable_qr {
            if let Some(qr) = self.try_decode_qr_v1_all_levels_with_correction(img) {
                out.push(qr);
            }
        }

        dedup_by_sym_and_text(out)
    }

    /// Узконаправленный декодер QR v1:
    /// - ищем finder patterns,
    /// - семплим projective сетку 21×21,
    /// - читаем формат (EC и mask),
    /// - снимаем маску корректно (только с data-модулей),
    /// - проходим по маршруту v1, получаем 208 бит,
    /// - формируем кодворды, проверяем/корректируем RS,
    /// - парсим Byte mode (ожидаем «HELLO» в тесте).
    fn try_decode_qr_v1_all_levels_with_correction(
        &self,
        img: &LumaImage,
    ) -> Option<DecodedSymbol> {
        let qr_opts = QrOptions::default();

        // 1) Finder patterns
        let finders = finder::find_finder_patterns(&img.as_gray(), &qr_opts);
        if finders.len() < 3 {
            return None;
        }

        // 2) Семплинг сетки 21×21 (flatten: Vec<bool> длиной 441).
        let grid: Vec<bool> = sample::sample_qr_v1_grid(&img.as_gray(), &qr_opts, &finders)?;

        // Матрица 21×21
        let mut matrix: Vec<Vec<bool>> = vec![vec![false; data::N1]; data::N1];
        for y in 0..data::N1 {
            for x in 0..data::N1 {
                matrix[y][x] = grid[y * data::N1 + x];
            }
        }

        // 3) Формат (две копии по 15 бит) → (ec_level, mask, ...).
        let (ec_level, mask_id, _hamming_dist, _src_index) =
            qr::decode_v1_format_from_matrix(&matrix)?;
        println!(
            "[qr] format OK: ec={} mask={}",
            ec_level_to_str(ec_level),
            mask_id
        );

        // Белый список уровней EC (если непустой).
        if !self.opts.qr_allowed_ec_levels.is_empty()
            && !self
                .opts
                .qr_allowed_ec_levels
                .iter()
                .any(|&v| v == ec_level)
        {
            return None;
        }

        // 4) Снять маску только с data-модулей.
        let unmask = unmask_matrix_v1(&matrix, mask_id);

        // 5) В плоский вектор
        let mut flat: Vec<bool> = Vec::with_capacity(data::N1 * data::N1);
        for y in 0..data::N1 {
            for x in 0..data::N1 {
                flat.push(unmask[y][x]);
            }
        }

        // 6) Извлечь 208 data-бит (для v1 — фиксированная схема обхода).
        let data_bits: Vec<bool> = data::extract_data_bits_v1(&flat);

        // 7) Разное разбиение 26 кодвордов для уровней L/M/Q/H:
        let (data_len, ec_len) = match ec_level {
            format::EcLevel::L => (19usize, 7usize),
            format::EcLevel::M => (16usize, 10usize),
            format::EcLevel::Q => (13usize, 13usize),
            format::EcLevel::H => (9usize, 17usize),
        };

        // 8) 208 бит → 26 байт кодвордов (MSB первым в байте).
        if data_bits.len() != 208 {
            println!("[qr] unexpected data bits length: {}", data_bits.len());
            return None;
        }
        let mut codewords: Vec<u8> = Vec::with_capacity(26);
        for i in 0..26 {
            let mut b = 0u8;
            for j in 0..8 {
                if data_bits[i * 8 + j] {
                    b |= 1 << (7 - j);
                }
            }
            codewords.push(b);
        }

        // Оригинальные кодворды (для сравнения/логов).
        let cw_orig = codewords.clone();
        let mut cw = codewords;

        let mut extras = DecodedExtras::new()
            .with("qr.ec", ec_level_to_str(ec_level))
            .with("qr.mask", mask_id.to_string());

        // 9) Проверка RS «как есть».
        let mut rs_match = false;
        if self.opts.qr_verify_rs {
            let (d, e) = cw_orig.split_at(data_len);
            let calc = rs::rs_ec_bytes(d, ec_len);
            rs_match = calc == e;
            println!(
                "[qr] RS check (pre-correction): match={} (have={} calc={})",
                rs_match,
                hex_bytes(e),
                hex_bytes(&calc)
            );
            extras = extras.with("qr.rs_match", if rs_match { "true" } else { "false" });
        }

        // 10) Попытка исправить ошибки *in-place*.
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

        // 11) Парсим Byte-mode из ИСПРАВЛЕННЫХ кодвордов (если коррекция не удалась,
        // cw == cw_orig — парсим исходное).
        let bits_from_cw = bytes_to_bits_msb(&cw);
        let text: String = match bytes::parse_byte_mode_bits_v1_l(&bits_from_cw) {
            Some(t) => t,
            None => return None,
        };

        // 12) Итоговая уверенность (эвристика).
        let mut confidence = 0.80;
        // за более высокий уровень EC — чуть выше уверенность
        confidence += match ec_level {
            format::EcLevel::L => 0.00,
            format::EcLevel::M => 0.02,
            format::EcLevel::Q => 0.03,
            format::EcLevel::H => 0.05,
        };
        if self.opts.qr_verify_rs && rs_match {
            confidence += 0.10;
        }
        if corrected_bytes > 0 {
            confidence += 0.05;
        }
        if confidence > 0.99 {
            confidence = 0.99;
        }

        println!(
            "[qr] OK: text=\"{}\" ec={} mask={} corrected_bytes={}",
            text,
            ec_level_to_str(ec_level),
            mask_id,
            corrected_bytes
        );

        Some(
            DecodedSymbol::new(Symbology::QR, text)
                .with_confidence(confidence)
                .with_extras(extras),
        )
    }
}

/// Снять маску `mask_id` (0..7) — вернёт новую матрицу 21×21 с XOR маской.
/// ВАЖНО: маска применяется ТОЛЬКО к data-модулям, а не к function patterns.
fn unmask_matrix_v1(matrix: &[Vec<bool>], mask_id: u8) -> Vec<Vec<bool>> {
    let n = data::N1;
    let mut out = matrix.to_vec(); // Start with a copy
    for y in 0..n {
        for x in 0..n {
            if !data::is_function_v1(x, y) {
                let m = data::mask_predicate(mask_id, x, y);
                out[y][x] ^= m;
            }
        }
    }
    out
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

/// Утилита для логов: байты → hex-строка.
fn hex_bytes(bs: &[u8]) -> String {
    let mut s = String::with_capacity(bs.len() * 2);
    for b in bs {
        use std::fmt::Write as _;
        let _ = write!(&mut s, "{:02X}", b);
    }
    s
}
