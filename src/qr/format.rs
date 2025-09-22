//! QR v1: форматная информация (EC уровень + mask id) и размаскировка матрицы.

use super::data::is_function_v1;

/// Уровень коррекции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcLevel { L, M, Q, H }

impl EcLevel {
    #[inline] fn to_bits(self) -> u16 {
        // Стандарт: M=00, L=01, H=10, Q=11 (именно в таком коде)
        match self {
            EcLevel::M => 0b00,
            EcLevel::L => 0b01,
            EcLevel::H => 0b10,
            EcLevel::Q => 0b11,
        }
    }
}

/// Декодировать (EC, mask_id) из 21×21 матрицы (row-major).
/// Возвращает None, если обе копии format-инфо не распознаны (<- слишком много ошибок).
pub fn decode_format_info_v1(grid: &[bool]) -> Option<(EcLevel, u8)> {
    let a = read_format_15bits_copy_a(grid);
    let b = read_format_15bits_copy_b(grid);

    if let Some((ec, m, _d)) = best_match_format(a) { return Some((ec, m)); }
    if let Some((ec, m, _d)) = best_match_format(b) { return Some((ec, m)); }
    None
}

/// Снять маску `mask_id` со всех **данных** модулей (служебные не трогаем).
pub fn unmask_grid_v1(grid: &mut [bool], mask_id: u8) {
    assert_eq!(grid.len(), 21*21);
    for y in 0..21 {
        for x in 0..21 {
            if is_function_v1(x, y) { continue; }
            if mask_hit(mask_id, x, y) {
                let idx = y*21 + x;
                grid[idx] = !grid[idx];
            }
        }
    }
}

// === Ниже — внутренности: чтение двух копий, сравнение со всеми 32 вариантами и маски ===

const BCH_FORMAT_GEN: u16 = 0b10100110111;  // G(x) для (15,5)
const FORMAT_MASK:   u16 = 0b101010000010010; // 0x5412

fn best_match_format(read15: u16) -> Option<(EcLevel, u8, u32)> {
    // Перебор 32 значений (EC×mask) — находим минимальную Хэммингову дистанцию (<=3).
    let mut best: Option<(EcLevel, u8, u32)> = None;
    for &ec in &[EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H] {
        for mask in 0u8..=7u8 {
            let code = format_bits_masked(ec, mask);
            let d = (code ^ read15).count_ones();
            match best {
                None => best = Some((ec, mask, d)),
                Some((_,_,bd)) if d < bd => best = Some((ec, mask, d)),
                _ => {}
            }
        }
    }
    match best {
        Some((ec, m, d)) if d <= 3 => Some((ec, m, d)), // QR гарантирует исправление до 3 бит
        _ => None
    }
}

/// Сформировать 15-битный format code (с BCH и XOR-маской).
#[inline]
fn format_bits_masked(ec: EcLevel, mask_id: u8) -> u16 {
    let info: u16 = (ec.to_bits() << 3) | ((mask_id as u16) & 0b111);
    let code = bch15_5_encode(info);
    code ^ FORMAT_MASK
}

/// BCH(15,5): (info<<10) + остаток по модулю генератора.
fn bch15_5_encode(info5: u16) -> u16 {
    debug_assert!(info5 < (1<<5));
    let mut v = info5 << 10;
    let mut msb = 14; // старший бит в 15-битном слове
    while msb >= 10 {
        if (v & (1<<msb)) != 0 {
            v ^= BCH_FORMAT_GEN << (msb-10);
        }
        if msb == 0 { break; }
        msb -= 1;
    }
    (info5 << 10) | (v & 0x03FF)
}

/// Первая копия (вокруг TL): 15 модулей по строке y=8 (x=0..=8, x!=6) + по колонке x=8 (y=8..=0, y!=6), без (8,8) дубля.
fn read_format_15bits_copy_a(grid: &[bool]) -> u16 {
    let mut bits: u16 = 0;
    let mut push = |b: bool| { bits = (bits << 1) | (b as u16); };
    // y=8, x=0..=8, x!=6
    for x in 0..=8 {
        if x == 6 { continue; }
        push(grid[8*21 + x]);
    }
    // x=8, y=8..=0, y!=8, y!=6  (чтобы (8,8) не задублировать)
    for y in (0..=8).rev() {
        if y == 8 || y == 6 { continue; }
        push(grid[y*21 + 8]);
    }
    bits
}

/// Вторая копия (TR/BL): x=20..=13 на y=8 (8 модулей) + y=20..=14 на x=8 (7 модулей, исключая y=13 — dark module).
fn read_format_15bits_copy_b(grid: &[bool]) -> u16 {
    let mut bits: u16 = 0;
    let mut push = |b: bool| { bits = (bits << 1) | (b as u16); };
    // y=8, x=20..=13
    for x in (13..=20).rev() {
        push(grid[8*21 + x]);
    }
    // x=8, y=20..=14 (13 пропускаем — там dark module)
    for y in (14..=20).rev() {
        push(grid[y*21 + 8]);
    }
    bits
}

/// Маска M0..M7. Параметры — координаты модулей (x=column, y=row).
#[inline]
fn mask_hit(mask_id: u8, x: usize, y: usize) -> bool {
    let x = x as i32; let y = y as i32;
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

#[cfg(test)]
pub fn encode_format_bits_for_tests(ec: EcLevel, mask_id: u8) -> u16 {
    format_bits_masked(ec, mask_id)
}
