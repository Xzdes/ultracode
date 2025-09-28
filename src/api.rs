//! High-level decoding pipeline for QR v1 (+ подробные логи).

use crate::prelude::*; // DecodedSymbol, GrayImage, LumaImage, Symbology, Orientation, DecodedExtras, Quad
use crate::qr::{bytes, data, finder, format, rs, sample, QrOptions};
use std::collections::BTreeMap;

const N1: usize = 21;

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub qr_opts: QrOptions,

    /// Флаги включения/выключения семейств символогий (ожидаются тестами).
    pub ean13_upca_enabled: bool,
    pub code128_enabled: bool,
    pub qr_enabled: bool,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self {
            qr_opts: QrOptions::default(),
            ean13_upca_enabled: true,
            code128_enabled: true,
            qr_enabled: true,
        }
    }
}

/// Строитель для Pipeline, чтобы соответствовать ожидаемому внешнему API.
#[derive(Clone, Debug, Default)]
pub struct PipelineBuilder {
    qr_opts: QrOptions,
    ean13_upca_enabled: bool,
    code128_enabled: bool,
    qr_enabled: bool,
}

impl PipelineBuilder {
    /// Создать новый билдер с настройками по умолчанию.
    pub fn new() -> Self {
        Self {
            qr_opts: QrOptions::default(),
            ean13_upca_enabled: true,
            code128_enabled: true,
            qr_enabled: true,
        }
    }

    /// Задать опции QR.
    pub fn with_qr_opts(mut self, qr_opts: QrOptions) -> Self {
        self.qr_opts = qr_opts;
        self
    }

    /// Включить/выключить поддержку EAN-13/UPC-A (ожидается тестами).
    pub fn enable_ean13_upca(mut self, enabled: bool) -> Self {
        self.ean13_upca_enabled = enabled;
        self
    }

    /// Включить/выключить поддержку Code128 (ожидается тестами).
    pub fn enable_code128(mut self, enabled: bool) -> Self {
        self.code128_enabled = enabled;
        self
    }

    /// Включить/выключить поддержку QR (ожидается тестами).
    pub fn enable_qr(mut self, enabled: bool) -> Self {
        self.qr_enabled = enabled;
        self
    }

    /// Построить Pipeline.
    pub fn build(self) -> Pipeline {
        Pipeline {
            qr_opts: self.qr_opts,
            ean13_upca_enabled: self.ean13_upca_enabled,
            code128_enabled: self.code128_enabled,
            qr_enabled: self.qr_enabled,
        }
    }
}

impl Pipeline {
    /// Декодировать все символы из LumaImage (как ожидают интеграционные тесты).
    pub fn decode_all(&self, img: &LumaImage) -> Vec<DecodedSymbol> {
        // В будущем можно разветвить по флагам:
        // if self.qr_enabled { ... } / if self.ean13_upca_enabled { ... } / if self.code128_enabled { ... }
        let gray = img.as_gray();
        self.decode_all_gray(&gray)
    }

    /// Вспомогательный метод: декодирование из GrayImage (внутренний).
    fn decode_all_gray<'a>(&self, img: &GrayImage<'a>) -> Vec<DecodedSymbol> {
        println!("[pipeline] ===== decode_all start =====");

        // 1) Поиск finder patterns
        let pts = finder::find_finder_patterns(img, &self.qr_opts);
        println!("[finder] found={} points:", pts.len());
        for (i, p) in pts.iter().enumerate() {
            println!("  [{:02}] ({:.2},{:.2})", i, p.x, p.y);
        }
        if pts.len() < 3 {
            println!("[finder] not enough points (<3).");
            return Vec::new();
        }

        // Возьмём 3 лучших по эвристике: самые верхние (малое y) — TL/TR, самый нижний — BL
        let mut three = pts[0..3].to_vec();
        three.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap()); // по y (сверху вниз)

        let mut tl = three[0];
        let mut tr = three[1];
        let mut bl = three[2];

        // из верхней пары слева должен быть TL
        if tl.x > tr.x {
            std::mem::swap(&mut tl, &mut tr);
        }

        // если BL вдруг не снизу — подберём самый нижний из всех
        if bl.y < tl.y.max(tr.y) {
            bl = *pts
                .iter()
                .max_by(|a, b| a.y.partial_cmp(&b.y).unwrap())
                .unwrap();
        }

        println!(
            "[finder] pick TL=({:.2},{:.2}) TR=({:.2},{:.2}) BL=({:.2},{:.2})",
            tl.x, tl.y, tr.x, tr.y, bl.x, bl.y
        );

        // 2) Сэмплинг сетки 21x21
        let finders = [tl, tr, bl];
        let Some(grid) = sample::sample_qr_v1_grid(img, &self.qr_opts, &finders) else {
            println!("[sample] sample_qr_v1_grid failed.");
            return Vec::new();
        };
        println!("[sample] grid sampled: {} bits ({}x{})", grid.len(), N1, N1);

        // 3) Чтение format (EC + mask): пробуем обычную и инвертированную матрицу
        let fmt_normal = read_format_from_grid(&grid, "normal");
        let inv_grid = invert_grid(&grid);
        let fmt_inverted = read_format_from_grid(&inv_grid, "inverted");

        let (use_inverted, (ec, mask_id, ham_best)) = match (fmt_normal, fmt_inverted) {
            (Some(a), Some(b)) => {
                let pick = if a.2 <= b.2 { a } else { b };
                let inv = a.2 > b.2;
                println!(
                    "[format] both candidates present; pick={} (ham={}), other ham={}",
                    if inv { "inverted" } else { "normal" },
                    pick.2,
                    if inv { a.2 } else { b.2 }
                );
                (inv, pick)
            }
            (Some(a), None) => {
                println!("[format] only normal present (ham={})", a.2);
                (false, a)
            }
            (None, Some(b)) => {
                println!("[format] only inverted present (ham={})", b.2);
                (true, b)
            }
            (None, None) => {
                println!("[format] FAILED: no valid format word.");
                return Vec::new();
            }
        };

        println!(
            "[format] OK: ec={:?} mask={} inverted={} ham={}",
            ec, mask_id, use_inverted, ham_best
        );

        // 4) Размаскировка только data-модулей
        let sampled = if use_inverted { inv_grid } else { grid };
        let unmasked = unmask_grid_v1(&sampled, mask_id);
        println!("[unmask] done.");

        // 5) Извлечь дата-биты и собрать кодовые слова
        let bits_data = data::extract_data_bits_v1(&unmasked);
        println!("[data] bits extracted: {}", bits_data.len());
        let mut codewords = bytes::bits_to_bytes_v1(&bits_data);
        println!("[data] codewords assembled: {}", codewords.len());

        // 6) RS коррекция для V1
        let (data_len, ec_len) = v1_block_layout(ec);
        println!(
            "[rs] layout (V1, ec={:?}): data_len={} ec_len={}",
            ec, data_len, ec_len
        );
        if codewords.len() < data_len + ec_len {
            println!(
                "[rs] not enough codewords: have={}, need={}",
                codewords.len(),
                data_len + ec_len
            );
            return Vec::new();
        }
        match rs::rs_correct_codeword_block(&mut codewords, data_len, ec_len) {
            Ok(corrected) => println!("[rs] correction OK, corrected symbols: {}", corrected),
            Err(_) => println!("[rs] correction reported error (continuing with best effort)"),
        }

        // 7) Парсинг полезной нагрузки:
        // helper принимает биты, поэтому построим битовый поток из исправленных *data* кодовых слов
        let mut data_bits_after_rs: Vec<bool> = Vec::with_capacity(data_len * 8);
        for &b in &codewords[..data_len] {
            for i in (0..8).rev() {
                data_bits_after_rs.push(((b >> i) & 1) != 0);
            }
        }

        let text_opt = bytes::parse_byte_mode_bits_v1_l(&data_bits_after_rs);
        if let Some(text) = text_opt {
            let mut props = BTreeMap::new();
            props.insert("qr.ec".into(), ec_to_str(ec).into());
            props.insert("qr.mask_id".into(), mask_id.to_string());
            props.insert("qr.inverted".into(), use_inverted.to_string());

            let sym = DecodedSymbol {
                text, // String
                bytes: None,
                symbology: Symbology::QR,
                quad: None::<Quad>,
                orientation: None::<Orientation>,
                confidence: 0.85,
                extras: DecodedExtras { properties: props },
            };
            println!("[parse] TEXT='{}'", sym.text);
            println!("[pipeline] ===== decode_all done: 1 hit =====");
            return vec![sym];
        } else {
            println!("[parse] payload parse FAILED.");
            println!("[pipeline] ===== decode_all done: 0 hits =====");
            return Vec::new();
        }
    }

    /// Удобный хелпер — вернуть первый найденный символ (из LumaImage).
    pub fn decode_first(&self, img: &LumaImage) -> Option<DecodedSymbol> {
        let gray = img.as_gray();
        let mut v = self.decode_all_gray(&gray);
        v.pop()
    }
}

/// Чтение формат-слова из сетки с путями `FORMAT_READ_PATHS_V1`.
/// Возвращает (EcLevel, mask_id, hamming_distance).
fn read_format_from_grid(
    grid: &[bool],
    tag: &str,
) -> Option<(format::EcLevel, u8, u32)> {
    use format::{decode_format_word, FORMAT_READ_PATHS_V1};
    let mut best: Option<(format::EcLevel, u8, u32)> = None;

    for (pi, path) in FORMAT_READ_PATHS_V1.iter().enumerate() {
        let mut word: u16 = 0;
        for &(x, y) in path.iter() {
            word <<= 1;
            if get(grid, x, y) {
                word |= 1;
            }
        }
        if let Some((ec, mask_id, ham)) = decode_format_word(word) {
            println!(
                "[format:{}] path#{} word=0x{:04X} -> ec={:?} mask={} ham={}",
                tag, pi, word, ec, mask_id, ham
            );
            best = match best {
                None => Some((ec, mask_id, ham)),
                Some((_, _, h0)) if ham < h0 => Some((ec, mask_id, ham)),
                Some(prev) => Some(prev),
            };
        } else {
            println!(
                "[format:{}] path#{} word=0x{:04X} -> decode FAIL",
                tag, pi, word
            );
        }
    }
    best
}

/// Инвертирование сетки (битов).
#[inline]
fn invert_grid(grid: &[bool]) -> Vec<bool> {
    grid.iter().map(|&b| !b).collect()
}

/// Получить бит из сетки 21x21.
#[inline]
fn get(grid: &[bool], x: usize, y: usize) -> bool {
    grid[y * N1 + x]
}

/// Снять маску только с data-модулей (functional модули не трогаем).
fn unmask_grid_v1(grid: &[bool], mask_id: u8) -> Vec<bool> {
    let mut out = grid.to_vec();
    for y in 0..N1 {
        for x in 0..N1 {
            if !data::is_function_v1(x, y) && data::mask_predicate(mask_id, x as i32, y as i32) {
                let i = y * N1 + x;
                out[i] = !out[i];
            }
        }
    }
    out
}

/// Длины для версии 1.
fn v1_block_layout(ec: format::EcLevel) -> (usize, usize) {
    match ec {
        format::EcLevel::L => (19, 7),
        format::EcLevel::M => (16, 10),
        format::EcLevel::Q => (13, 13),
        format::EcLevel::H => (9, 17),
    }
}

fn ec_to_str(ec: format::EcLevel) -> &'static str {
    match ec {
        format::EcLevel::L => "L",
        format::EcLevel::M => "M",
        format::EcLevel::Q => "Q",
        format::EcLevel::H => "H",
    }
}
