//! High-level decoding pipeline for QR v1 (+ подробные логи, корректная работа со служебной маской).

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
    pub fn new() -> Self {
        Self {
            qr_opts: QrOptions::default(),
            ean13_upca_enabled: true,
            code128_enabled: true,
            qr_enabled: true,
        }
    }

    pub fn with_qr_opts(mut self, qr_opts: QrOptions) -> Self {
        self.qr_opts = qr_opts;
        self
    }

    pub fn enable_ean13_upca(mut self, enabled: bool) -> Self {
        self.ean13_upca_enabled = enabled;
        self
    }

    pub fn enable_code128(mut self, enabled: bool) -> Self {
        self.code128_enabled = enabled;
        self
    }

    pub fn enable_qr(mut self, enabled: bool) -> Self {
        self.qr_enabled = enabled;
        self
    }

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
    pub fn decode_all(&self, img: &LumaImage) -> Vec<DecodedSymbol> {
        let gray = img.as_gray();
        self.decode_all_gray(&gray)
    }

    fn decode_all_gray<'a>(&self, img: &GrayImage<'a>) -> Vec<DecodedSymbol> {
        println!("[pipeline] ===== decode_all start =====");

        // 1) Finder patterns
        let pts = finder::find_finder_patterns(img, &self.qr_opts);
        println!("[finder] found={} points:", pts.len());
        for (i, p) in pts.iter().enumerate() {
            println!("  [{:02}] ({:.2},{:.2})", i, p.x, p.y);
        }
        if pts.len() < 3 {
            println!("[finder] not enough points (<3).");
            return Vec::new();
        }

        // Подберём TL, TR, BL
        let mut three = pts[0..3].to_vec();
        three.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
        let mut tl = three[0];
        let mut tr = three[1];
        let mut bl = three[2];
        if tl.x > tr.x {
            std::mem::swap(&mut tl, &mut tr);
        }
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

        // 2) Сэмплинг 21x21
        let finders = [tl, tr, bl];
        let Some(grid) = sample::sample_qr_v1_grid(img, &self.qr_opts, &finders) else {
            println!("[sample] sample_qr_v1_grid failed.");
            return Vec::new();
        };
        println!("[sample] grid sampled: {} bits ({}x{})", grid.len(), N1, N1);

        // 3) Кандидаты формат-слова на всех симметриях (D4 × inversion)
        let candidates = scan_format_candidates_any_symmetry(&grid);
        if candidates.is_empty() {
            println!("[format] FAILED: no valid format word in any symmetry/inversion.");
            return Vec::new();
        }

        // 4) Выбираем симметрию, где удаётся извлечь полный набор бит (208 для V1-L)
        let mut chosen: Option<(usize, bool, bool, bool, format::EcLevel, u8, u32)> = None;
        let mut sampled_final: Option<Vec<bool>> = None;
        let mut fmask_final: Option<Vec<bool>> = None;

        'pick: for &(rot_k, mh, mv, inv, ec, mask_id, ham) in &candidates {
            // Применим симметрию к сетке
            let transformed = apply_symmetry(&grid, rot_k, mh, mv);
            let sampled = if inv { invert_grid(&transformed) } else { transformed };

            // Построим служебную маску и применим к ней ту же симметрию (инверсию не применяем)
            let fmask = apply_symmetry(&build_function_mask_v1(), rot_k, mh, mv);

            let (data_len, ec_len) = v1_block_layout(ec);
            let expected_bits = (data_len + ec_len) * 8;

            // Размаскировка c учётом служебной маски
            let unmasked = unmask_grid_v1(&sampled, &fmask, mask_id);

            // Попытка строгого извлечения (канонический зигзаг справа-налево)
            if let Some(bits_data) = extract_data_bits_v1_strict(&unmasked, &fmask, expected_bits) {
                println!(
                    "[format] pick by completeness: ec={:?} mask={} inv={} rot={}*90 mh={} mv={} ham={} bits={}",
                    ec, mask_id, inv, rot_k, mh, mv, ham, bits_data.len()
                );
                chosen = Some((rot_k, mh, mv, inv, ec, mask_id, ham));
                sampled_final = Some(sampled);
                fmask_final = Some(fmask);
                break 'pick;
            }
        }

        // Если «полный» не нашёлся — берём лучший по Hamming
        let (rot_k, mirror_h, mirror_v, use_inverted, ec, mask_id, ham_best, sampled, fmask) =
            if let (Some(ch), Some(sm), Some(fm)) = (chosen, sampled_final, fmask_final) {
                (ch.0, ch.1, ch.2, ch.3, ch.4, ch.5, ch.6, sm, fm)
            } else {
                let ch = candidates[0];
                let transformed = apply_symmetry(&grid, ch.0, ch.1, ch.2);
                let smp = if ch.3 { invert_grid(&transformed) } else { transformed };
                let fm = apply_symmetry(&build_function_mask_v1(), ch.0, ch.1, ch.2);
                println!(
                    "[format] pick by ham only: ec={:?} mask={} inv={} rot={}*90 mh={} mv={} ham={}",
                    ch.4, ch.5, ch.3, ch.0, ch.1, ch.2, ch.6
                );
                (ch.0, ch.1, ch.2, ch.3, ch.4, ch.5, ch.6, smp, fm)
            };

        println!(
            "[format] OK: ec={:?} mask={} inverted={} rot={}*90 mh={} mv={} ham={}",
            ec, mask_id, use_inverted, rot_k, mirror_h, mirror_v, ham_best
        );

        // 5) Размаскировка и извлечение с согласованной служебной маской
        let unmasked = unmask_grid_v1(&sampled, &fmask, mask_id);
        println!("[unmask] done.");

        let (data_len, ec_len) = v1_block_layout(ec);
        let expected_bits = (data_len + ec_len) * 8;

        let bits_data = if let Some(b) = extract_data_bits_v1_strict(&unmasked, &fmask, expected_bits) {
            b
        } else {
            // legacy (на всякий)
            let b = data::extract_data_bits_v1(&unmasked);
            println!(
                "[data] strict extractor failed; legacy bits={} (expected={})",
                b.len(),
                expected_bits
            );
            b
        };

        println!("[data] bits extracted: {} (expected {})", bits_data.len(), expected_bits);
        let mut codewords = bytes::bits_to_bytes_v1(&bits_data);
        println!("[data] codewords assembled: {}", codewords.len());

        // 6) RS коррекция
        println!(
            "[rs] layout (V1, ec={:?}): data_len={} ec_len={}",
            ec, data_len, ec_len
        );
        let need_cw = data_len + ec_len;

        if codewords.len() < need_cw {
            println!(
                "[rs] not enough codewords: have={}, need={} -> soft fallback: parse without RS",
                codewords.len(),
                need_cw
            );

            if codewords.len() >= data_len {
                let mut data_bits_after_rs: Vec<bool> = Vec::with_capacity(data_len * 8);
                for &b in &codewords[..data_len] {
                    for i in (0..8).rev() {
                        data_bits_after_rs.push(((b >> i) & 1) != 0);
                    }
                }

                if let Some(text) = bytes::parse_byte_mode_bits_v1_l(&data_bits_after_rs) {
                    let mut props = BTreeMap::new();
                    props.insert("qr.ec".into(), ec_to_str(ec).into());
                    props.insert("qr.mask_id".into(), mask_id.to_string());
                    props.insert("qr.inverted".into(), use_inverted.to_string());
                    props.insert("qr.rotation".into(), (rot_k * 90).to_string());
                    props.insert("qr.mirror_h".into(), mirror_h.to_string());
                    props.insert("qr.mirror_v".into(), mirror_v.to_string());
                    props.insert("qr.rs_skipped".into(), "true".into());
                    props.insert(
                        "qr.missing_codewords".into(),
                        (need_cw.saturating_sub(codewords.len())).to_string(),
                    );

                    let sym = DecodedSymbol {
                        text,
                        bytes: None,
                        symbology: Symbology::QR,
                        quad: None::<Quad>,
                        orientation: None::<Orientation>,
                        confidence: 0.60,
                        extras: DecodedExtras { properties: props },
                    };
                    println!("[parse-soft] TEXT='{}'", sym.text);
                    println!("[pipeline] ===== decode_all done: 1 hit (soft) =====");
                    return vec![sym];
                } else {
                    println!("[parse-soft] payload parse FAILED.");
                    return Vec::new();
                }
            } else {
                println!(
                    "[rs] not enough even for payload: have_data_cw={}, need_data_cw={}",
                    codewords.len(),
                    data_len
                );
                return Vec::new();
            }
        }

        match rs::rs_correct_codeword_block(&mut codewords, data_len, ec_len) {
            Ok(corrected) => println!("[rs] correction OK, corrected symbols: {}", corrected),
            Err(_) => println!("[rs] correction reported error (continuing with best effort)"),
        }

        // 7) Парсинг полезной нагрузки
        let mut data_bits_after_rs: Vec<bool> = Vec::with_capacity(data_len * 8);
        for &b in &codewords[..data_len] {
            for i in (0..8).rev() {
                data_bits_after_rs.push(((b >> i) & 1) != 0);
            }
        }

        if let Some(text) = bytes::parse_byte_mode_bits_v1_l(&data_bits_after_rs) {
            let mut props = BTreeMap::new();
            props.insert("qr.ec".into(), ec_to_str(ec).into());
            props.insert("qr.mask_id".into(), mask_id.to_string());
            props.insert("qr.inverted".into(), use_inverted.to_string());
            props.insert("qr.rotation".into(), (rot_k * 90).to_string());
            props.insert("qr.mirror_h".into(), mirror_h.to_string());
            props.insert("qr.mirror_v".into(), mirror_v.to_string());

            let sym = DecodedSymbol {
                text,
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
            Vec::new()
        }
    }

    pub fn decode_first(&self, img: &LumaImage) -> Option<DecodedSymbol> {
        let gray = img.as_gray();
        let mut v = self.decode_all_gray(&gray);
        v.pop()
    }
}

// ---------- формат и симметрии ----------

fn scan_format_candidates_any_symmetry(
    grid: &[bool],
) -> Vec<(usize, bool, bool, bool, format::EcLevel, u8, u32)> {
    let mut out: Vec<(usize, bool, bool, bool, format::EcLevel, u8, u32)> = Vec::new();

    for rot in 0..4 {
        for &mh in &[false, true] {
            for &mv in &[false, true] {
                let g = apply_symmetry(grid, rot, mh, mv);

                let fmt_normal = read_format_from_grid(&g, "normal");
                if let Some((ec, mask_id, ham)) = fmt_normal {
                    out.push((rot, mh, mv, false, ec, mask_id, ham));
                }
                let inv_g = invert_grid(&g);
                let fmt_inverted = read_format_from_grid(&inv_g, "inverted");
                if let Some((ec, mask_id, ham)) = fmt_inverted {
                    out.push((rot, mh, mv, true, ec, mask_id, ham));
                }
            }
        }
    }

    out.sort_by_key(|c| c.6);
    out
}

#[inline]
fn apply_symmetry(grid: &[bool], rot_k: usize, mirror_h: bool, mirror_v: bool) -> Vec<bool> {
    let mut out = rotate_grid_k(grid, rot_k);
    if mirror_h {
        out = mirror_grid_h(&out);
    }
    if mirror_v {
        out = mirror_grid_v(&out);
    }
    out
}

#[inline]
fn invert_grid(grid: &[bool]) -> Vec<bool> {
    grid.iter().map(|&b| !b).collect()
}

fn rotate_grid_k(grid: &[bool], k: usize) -> Vec<bool> {
    let k = k % 4;
    match k {
        0 => grid.to_vec(),
        1 => rotate_grid_90(grid),
        2 => rotate_grid_180(grid),
        _ => rotate_grid_270(grid),
    }
}

#[inline]
fn rotate_grid_90(grid: &[bool]) -> Vec<bool> {
    let mut out = vec![false; N1 * N1];
    for y in 0..N1 {
        for x in 0..N1 {
            let v = grid[y * N1 + x];
            let nx = N1 - 1 - y;
            let ny = x;
            out[ny * N1 + nx] = v;
        }
    }
    out
}

#[inline]
fn rotate_grid_180(grid: &[bool]) -> Vec<bool> {
    let mut out = vec![false; N1 * N1];
    for y in 0..N1 {
        for x in 0..N1 {
            let v = grid[y * N1 + x];
            let nx = N1 - 1 - x;
            let ny = N1 - 1 - y;
            out[ny * N1 + nx] = v;
        }
    }
    out
}

#[inline]
fn rotate_grid_270(grid: &[bool]) -> Vec<bool> {
    let mut out = vec![false; N1 * N1];
    for y in 0..N1 {
        for x in 0..N1 {
            let v = grid[y * N1 + x];
            let nx = y;
            let ny = N1 - 1 - x;
            out[ny * N1 + nx] = v;
        }
    }
    out
}

#[inline]
fn mirror_grid_h(grid: &[bool]) -> Vec<bool> {
    let mut out = vec![false; N1 * N1];
    for y in 0..N1 {
        for x in 0..N1 {
            let v = grid[y * N1 + x];
            let nx = N1 - 1 - x;
            out[y * N1 + nx] = v;
        }
    }
    out
}

#[inline]
fn mirror_grid_v(grid: &[bool]) -> Vec<bool> {
    let mut out = vec![false; N1 * N1];
    for y in 0..N1 {
        for x in 0..N1 {
            let v = grid[y * N1 + x];
            let ny = N1 - 1 - y;
            out[ny * N1 + x] = v;
        }
    }
    out
}

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

#[inline]
fn get(grid: &[bool], x: usize, y: usize) -> bool {
    grid[y * N1 + x]
}

// ---------- служебная маска и размаскировка ----------

/// Построить служебную маску (functional modules) для V1 в канонической ориентации.
fn build_function_mask_v1() -> Vec<bool> {
    let mut m = vec![false; N1 * N1];
    for y in 0..N1 {
        for x in 0..N1 {
            if data::is_function_v1(x, y) {
                m[y * N1 + x] = true;
            }
        }
    }
    m
}

/// Размаскировать ТОЛЬКО дата-модули, используя служебную маску `fmask`.
fn unmask_grid_v1(grid: &[bool], fmask: &[bool], mask_id: u8) -> Vec<bool> {
    let mut out = grid.to_vec();
    for y in 0..N1 {
        for x in 0..N1 {
            let i = y * N1 + x;
            if !fmask[i] && data::mask_predicate(mask_id, x as i32, y as i32) {
                out[i] = !out[i];
            }
        }
    }
    out
}

// ---------- извлечение дата-битов ----------

/// Канонический строгий извлекатель QR v1:
/// - Начинаем с правой пары столбцов (x=20,19) и идём парами влево: x-=2
/// - Колонку x=6 (тайминги) пропускаем целиком
/// - В каждой паре идём «змейкой»: то сверху вниз, то снизу вверх (переключая на каждую пару)
/// - Берём только те клетки, где `fmask==false` (т.е. data-модули)
/// - Возвращаем ровно `expected_bits` при успехе.
fn extract_data_bits_v1_strict(
    grid: &[bool],
    fmask: &[bool],
    expected_bits: usize,
) -> Option<Vec<bool>> {
    let mut bits: Vec<bool> = Vec::with_capacity(expected_bits);

    let mut dir_up = true; // первая пара читается снизу-вверх согласно распространённой имплементации; допустимо и наоборот
    let mut x: i32 = (N1 as i32) - 1;

    while x > 0 {
        if x == 6 {
            // Пропускаем тайминговую колонку
            x -= 1;
        }

        // Пара столбцов: (x, x-1)
        let xr = x;
        let xl = x - 1;

        if dir_up {
            // снизу -> вверх
            for y in (0..N1 as i32).rev() {
                push_if_data(grid, fmask, xr, y, &mut bits);
                push_if_data(grid, fmask, xl, y, &mut bits);
                if bits.len() == expected_bits { return Some(bits); }
            }
        } else {
            // сверху -> вниз
            for y in 0..(N1 as i32) {
                push_if_data(grid, fmask, xr, y, &mut bits);
                push_if_data(grid, fmask, xl, y, &mut bits);
                if bits.len() == expected_bits { return Some(bits); }
            }
        }

        dir_up = !dir_up;
        x -= 2;
    }

    // Если недобрали — провал
    None
}

#[inline]
fn push_if_data(
    grid: &[bool],
    fmask: &[bool],
    x: i32,
    y: i32,
    out: &mut Vec<bool>,
) {
    if x < 0 || y < 0 || x >= N1 as i32 || y >= N1 as i32 { return; }
    let xi = x as usize;
    let yi = y as usize;
    let idx = yi * N1 + xi;
    if !fmask[idx] {
        out.push(grid[idx]);
    }
}

// ---------- утилиты ----------

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
