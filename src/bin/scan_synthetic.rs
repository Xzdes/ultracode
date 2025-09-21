use std::env;
use ultracode::{GrayImage, decode_any, DecodeOptions};

fn main() {
    let mut code = String::from("5901234123457"); // валидный EAN-13 по умолчанию
    let mut unit: usize = 2;     // ширина модуля в пикселях
    let mut height: usize = 64;  // высота картинки
    let mut write_pgm: Option<String> = None;

    // Примитивный парсер аргументов:
    // --code 5901234123457  --unit 2  --height 64  --write-pgm out.pgm
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--code" => if let Some(v) = args.next() { code = v; },
            "--unit" => if let Some(v) = args.next() { unit = v.parse().unwrap_or(2); },
            "--height" => if let Some(v) = args.next() { height = v.parse().unwrap_or(64); },
            "--write-pgm" => if let Some(v) = args.next() { write_pgm = Some(v); },
            "--help" | "-h" => {
                print_help();
                return;
            }
            other => {
                eprintln!("Неизвестный аргумент: {other}");
                print_help();
                std::process::exit(2);
            }
        }
    }

    // Генерируем синтетическую строку пикселей (1 x N), чёрный=0, белый=255
    let row = synthesize_ean_row(&code, unit);
    let width = row.len();
    let mut img_buf = Vec::with_capacity(width * height);
    for _ in 0..height {
        img_buf.extend_from_slice(&row);
    }
    let img = GrayImage { width, height, data: &img_buf };

    // Запрос распознавания
    let opts = DecodeOptions::default();
    let results = decode_any(img, opts);

    if results.is_empty() {
        println!("Ничего не распознано :(");
    } else {
        for b in results {
            println!("{:?}: {}", b.format, b.text);
        }
    }

    // По необходимости сохраним PGM (P5)
    if let Some(path) = write_pgm {
        if let Err(e) = write_pgm_p5(&path, width, height, &img_buf) {
            eprintln!("Ошибка записи PGM: {e}");
        } else {
            println!("PGM сохранён: {}", path);
        }
    }
}

fn print_help() {
    eprintln!(
r#"Использование:
  cargo run --bin scan_synthetic -- [--code <digits>] [--unit <px>] [--height <px>] [--write-pgm <file.pgm>]

По умолчанию генерируется EAN-13 5901234123457 с unit=2 и height=64.

Примеры:
  cargo run --bin scan_synthetic --
  cargo run --bin scan_synthetic -- --code 036000291452   # UPC-A (12 цифр)
  cargo run --bin scan_synthetic -- --write-pgm out.pgm
"#
    );
}

// ======= Синтетический генератор EAN-13/UPC-A =======

// Шаблоны ширин модулей (1..4) для наборов A/B/C.
// По ширинам C == A; B — реверс A.
const A_PATTERNS: [(u8, u8, u8, u8); 10] = [
    (3,2,1,1),(2,2,2,1),(2,1,2,2),(1,4,1,1),(1,1,3,2),
    (1,2,3,1),(1,1,1,4),(1,3,1,2),(1,2,1,3),(3,1,1,2),
];
const B_PATTERNS: [(u8, u8, u8, u8); 10] = [
    (1,1,2,3),(1,2,2,2),(2,2,1,2),(1,1,4,1),(2,3,1,1),
    (1,3,2,1),(4,1,1,1),(2,1,3,1),(3,1,2,1),(2,1,1,3),
];
const C_PATTERNS: [(u8, u8, u8, u8); 10] = A_PATTERNS;

// Маски типов левых цифр (A=false / B=true) по первой цифре EAN-13.
const FIRST_DIGIT_MASKS: [(bool,bool,bool,bool,bool,bool); 10] = [
    (false,false,false,false,false,false), // 0
    (false,false,true ,false,true ,true ), // 1
    (false,false,true ,true ,false,true ), // 2
    (false,false,true ,true ,true ,false), // 3
    (false,true ,false,false,true ,true ), // 4
    (false,true ,true ,false,false,true ), // 5
    (false,true ,true ,true ,false,false), // 6
    (false,true ,false,true ,false,true ), // 7
    (false,true ,false,true ,true ,false), // 8
    (false,true ,true ,false,true ,false), // 9
];

// Реализация генерации: принимает вектор цифр (12 — UPC-A, 13 — EAN-13)
fn synthesize_ean_row_from_digits(digits: &[u8], unit: usize) -> Vec<u8> {
    let mut modules: Vec<u8> = Vec::new();
    // quiet zone (белый run)
    modules.push(9);
    // стартовый guard 101 (ч/б/ч) — три run-а
    modules.extend_from_slice(&[1,1,1]);

    // Нормализуем вход: поддерживаем 12 (UPC-A) или 13 (EAN-13) цифр.
    let mut ean13: [u8; 13] = [0; 13];
    match digits.len() {
        12 => {
            // UPC-A => EAN-13 с ведущим 0; пересчёт контрольной цифры
            ean13[0] = 0;
            for i in 0..12 { ean13[i+1] = digits[i]; }
            let mut sum = 0u32;
            for i in 0..12 {
                let w = if i % 2 == 0 { 1 } else { 3 };
                sum += ean13[i] as u32 * w;
            }
            ean13[12] = ((10 - (sum % 10)) % 10) as u8;
        }
        13 => {
            for i in 0..13 { ean13[i] = digits[i]; }
        }
        _ => panic!("Ожидались 12 (UPC-A) или 13 (EAN-13) цифр, получено {}", digits.len()),
    }

    let first = ean13[0] as usize;
    let mask = FIRST_DIGIT_MASKS[first];

    // Левая половина: 6 цифр, A/B (B — реверс A)
    for i in 0..6 {
        let d = ean13[1+i] as usize;
        let (a,b,c,dw) = if mask_at(mask, i) { B_PATTERNS[d] } else { A_PATTERNS[d] };
        modules.extend_from_slice(&[a,b,c,dw]);
    }

    // Центральный guard 01010 (5 run-ов, начиная с белого run-а)
    modules.extend_from_slice(&[1,1,1,1,1]);

    // Правая половина: 6 цифр, всегда набор C
    for i in 0..6 {
        let d = ean13[7+i] as usize;
        let (a,b,c,dw) = C_PATTERNS[d];
        modules.extend_from_slice(&[a,b,c,dw]);
    }

    // Финальный guard 101
    modules.extend_from_slice(&[1,1,1]);
    // Quiet zone (белый run)
    modules.push(9);

    // Превращаем модули в пиксели: начинаем с белого run-а тихой зоны
    let mut pix: Vec<u8> = Vec::new();
    let mut black = false;
    for m in modules {
        let w = m as usize * unit;
        let val = if black { 0u8 } else { 255u8 };
        for _ in 0..w { pix.push(val); }
        black = !black;
    }
    pix
}

// Удобная обёртка: принимает строку цифр и вызывает генератор для слайса цифр
fn synthesize_ean_row(code: &str, unit: usize) -> Vec<u8> {
    let digits: Vec<u8> = code.bytes().map(|c| {
        if !c.is_ascii_digit() { panic!("Код должен содержать только цифры"); }
        c - b'0'
    }).collect();
    synthesize_ean_row_from_digits(&digits, unit)
}

fn mask_at(mask: (bool,bool,bool,bool,bool,bool), idx: usize) -> bool {
    match idx {
        0 => mask.0, 1 => mask.1, 2 => mask.2, 3 => mask.3, 4 => mask.4, 5 => mask.5, _ => false
    }
}

// Простая запись PGM (P5, 8-бит)
fn write_pgm_p5(path: &str, width: usize, height: usize, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    write!(f, "P5\n{} {}\n255\n", width, height)?;
    f.write_all(data)?;
    Ok(())
}
