//! Полный синтез QR v1-L (Byte mode) в изображение: finders, timing, format, данные, маска.

use super::data::{is_function_v1, walk_pairs_v1};
use super::format::EcLevel;
use super::rs::rs_ec_bytes;
use crate::GrayImage;

// Локальная копия формат-энкодера и масок (чтобы не делать pub внутренним функциям).
const BCH_FORMAT_GEN: u16 = 0b10100110111;
const FORMAT_MASK: u16 = 0b101010000010010;
#[inline]
fn ec_to_bits(ec: EcLevel) -> u16 {
    match ec {
        EcLevel::M => 0b00,
        EcLevel::L => 0b01,
        EcLevel::H => 0b10,
        EcLevel::Q => 0b11,
    }
}
fn bch15_5_encode(info5: u16) -> u16 {
    let mut v = info5 << 10;
    let mut msb = 14;
    while msb >= 10 {
        if (v & (1 << msb)) != 0 {
            v ^= BCH_FORMAT_GEN << (msb - 10);
        }
        if msb == 0 {
            break;
        }
        msb -= 1;
    }
    (info5 << 10) | (v & 0x03FF)
}
#[inline]
fn encode_format_bits(ec: EcLevel, mask_id: u8) -> u16 {
    (bch15_5_encode((ec_to_bits(ec) << 3) | (mask_id as u16 & 7))) ^ FORMAT_MASK
}
#[inline]
fn mask_hit(mask_id: u8, x: usize, y: usize) -> bool {
    let x = x as i32;
    let y = y as i32;
    match mask_id {
        0 => (x + y) % 2 == 0,
        1 => (y % 2) == 0,
        2 => (x % 3) == 0,
        3 => (x + y) % 3 == 0,
        4 => ((y / 2) + (x / 3)) % 2 == 0,
        5 => ((x * y) % 2 + (x * y) % 3) == 0,
        6 => (((x * y) % 2) + ((x * y) % 3)) % 2 == 0,
        7 => (((x + y) % 2) + ((x * y) % 3)) % 2 == 0,
        _ => false,
    }
}

/// Построить валидный QR v1-L (Byte mode, один блок 19+7) и отрисовать как картинку (с quiet=4).
/// `mask_id` — 0..7. Для тестов удобно 3.
pub fn synthesize_qr_v1_from_text(text: &str, mask_id: u8, unit: usize) -> GrayImage<'static> {
    // 1) Собираем data codewords (19 байт): mode(4)=0100, len(8), payload, terminатор/паддинг.
    let bytes = text.as_bytes();
    assert!(
        bytes.len() <= 17,
        "v1-L Byte mode влезает до 17 байт данных"
    );
    let mut bits: Vec<bool> = Vec::new();
    // mode 0100
    for i in (0..4).rev() {
        bits.push(((0b0100 >> i) & 1) != 0);
    }
    // length 8
    for i in (0..8).rev() {
        bits.push((((bytes.len() as u32) >> i) & 1) != 0);
    }
    // payload
    for &b in bytes {
        for i in (0..8).rev() {
            bits.push(((b as u32 >> i) & 1) != 0);
        }
    }
    // terminator (до 4 нулей)
    let capacity_bits: usize = 19 * 8; // << фикс: явно usize
    let remaining = capacity_bits.saturating_sub(bits.len());
    let term = remaining.min(4);
    for _ in 0..term {
        bits.push(false);
    }
    // до байтовой границы
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    // Пад-кодворды 0xEC, 0x11
    let data_cw: Vec<u8> = {
        let mut out = Vec::new();
        for chunk in bits.chunks(8) {
            let mut b = 0u8;
            for &bit in chunk {
                b = (b << 1) | if bit { 1 } else { 0 };
            }
            out.push(b);
        }
        while out.len() < 19 {
            out.push(if out.len() % 2 == 0 { 0xEC } else { 0x11 });
        }
        out
    };

    // 2) ECC (7 байт), один блок → просто конкатенация.
    let ec = rs_ec_bytes(&data_cw, 7);
    let mut all_cw = Vec::with_capacity(26);
    all_cw.extend_from_slice(&data_cw);
    all_cw.extend_from_slice(&ec);

    // 3) Формируем матрицу 21×21 (false=белый, true=чёрный).
    let mut grid = vec![false; 21 * 21];

    // Finders (7×7) + вокруг белые (сепаратор) на фоне и quiet zone рисовать не надо тут.
    fn draw_finder(grid: &mut [bool], ox: usize, oy: usize) {
        for dy in 0..7 {
            for dx in 0..7 {
                let on_border = dx == 0 || dx == 6 || dy == 0 || dy == 6;
                let in_core = (dx >= 2 && dx <= 4) && (dy >= 2 && dy <= 4);
                let v = on_border || in_core;
                grid[(oy + dy) * 21 + (ox + dx)] = v;
            }
        }
    }
    draw_finder(&mut grid, 0, 0);
    draw_finder(&mut grid, 14, 0);
    draw_finder(&mut grid, 0, 14);

    // Timing row/col (везде, где это не finder/separator)
    for x in 0..21 {
        if x == 6 {
            continue;
        }
        if (0..=7).contains(&x) || (13..=20).contains(&x) { /* попадёт на сепараторы/формат — ок */
        }
        grid[6 * 21 + x] = (x % 2) == 0;
    }
    for y in 0..21 {
        if y == 6 {
            continue;
        }
        grid[y * 21 + 6] = (y % 2) == 0;
    }

    // Dark module
    grid[13 * 21 + 8] = true;

    // Format info (две копии), EC=L + mask_id
    let fmt = encode_format_bits(EcLevel::L, mask_id);
    // Copy A: y=8, x=0..=8 (кроме 6); x=8, y=8..=0 (кроме 8 и 6)
    {
        let mut bits15 = fmt;
        let mut put = |x: usize, y: usize| {
            let b = ((bits15 >> 14) & 1) != 0;
            grid[y * 21 + x] = b;
            bits15 <<= 1;
        };
        for x in 0..=8 {
            if x != 6 {
                put(x, 8);
            }
        }
        for y in (0..=8).rev() {
            if y != 8 && y != 6 {
                put(8, y);
            }
        }
    }
    // Copy B: y=8, x=20..=13; x=8, y=20..=14
    {
        let mut bits15 = fmt;
        let mut put = |x: usize, y: usize| {
            let b = ((bits15 >> 14) & 1) != 0;
            grid[y * 21 + x] = b;
            bits15 <<= 1;
        };
        for x in (13..=20).rev() {
            put(x, 8);
        }
        for y in (14..=20).rev() {
            put(8, y);
        }
    }

    // 4) Размещение данных по «змейке» с применением маски только для data-модулей.
    let mut bit_iter = all_cw
        .iter()
        .flat_map(|&cw| (0..8).rev().map(move |i| ((cw >> i) & 1) != 0));
    for (x, y) in walk_pairs_v1() {
        if is_function_v1(x, y) {
            continue;
        }
        if let Some(bit) = bit_iter.next() {
            grid[y * 21 + x] = bit ^ mask_hit(mask_id, x, y);
        }
    }

    // 5) В пиксели (quiet=4, unit px/модуль)
    let unit = unit.max(1);
    let qz = 4usize;
    let total = 21 + 2 * qz;
    let w = total * unit;
    let h = total * unit;
    let mut data = Vec::with_capacity(w * h);
    for my in 0..total {
        for _sy in 0..unit {
            for mx in 0..total {
                let val = if (qz..qz + 21).contains(&mx) && (qz..qz + 21).contains(&my) {
                    let gx = mx - qz;
                    let gy = my - qz;
                    grid[gy * 21 + gx]
                } else {
                    false
                }; // quiet = белый
                let px = if val { 0u8 } else { 255u8 };
                for _sx in 0..unit {
                    data.push(px);
                }
            }
        }
    }
    let boxed = data.into_boxed_slice();
    let leaked: &'static [u8] = Box::leak(boxed);
    GrayImage {
        width: w,
        height: h,
        data: leaked,
    }
}
