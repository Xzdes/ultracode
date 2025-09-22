use std::env;
use ultracode::{decode_any, synthesize_row_code128, DecodeOptions, GrayImage};

fn main() {
    let mut text = String::from("HELLO-128");
    let mut set = 'B';
    let mut unit: usize = 2;
    let mut height: usize = 64;
    let mut write_pgm: Option<String> = None;

    // Аргументы:
    // --text "HELLO-128"  --set B|A|C  --unit 2  --height 64  --write-pgm out.pgm
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--text" => {
                if let Some(v) = args.next() {
                    text = v;
                }
            }
            "--set" => {
                if let Some(v) = args.next() {
                    set = v.chars().next().unwrap_or('B');
                }
            }
            "--unit" => {
                if let Some(v) = args.next() {
                    unit = v.parse().unwrap_or(2);
                }
            }
            "--height" => {
                if let Some(v) = args.next() {
                    height = v.parse().unwrap_or(64);
                }
            }
            "--write-pgm" => {
                if let Some(v) = args.next() {
                    write_pgm = Some(v);
                }
            }
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

    let row = synthesize_row_code128(&text, set, unit);
    let width = row.len();
    let mut img_buf = Vec::with_capacity(width * height);
    for _ in 0..height {
        img_buf.extend_from_slice(&row);
    }
    let img = GrayImage {
        width,
        height,
        data: &img_buf,
    };

    let opts = DecodeOptions::default();
    let results = decode_any(img, opts);

    if results.is_empty() {
        println!("Ничего не распознано :(");
    } else {
        for b in results {
            println!("{:?}: {}", b.format, b.text);
        }
    }

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
  cargo run --bin scan_code128_synth -- [--text <ASCII>] [--set A|B|C] [--unit <px>] [--height <px>] [--write-pgm <file.pgm>]

По умолчанию генерируется Code128-B "HELLO-128" с unit=2 и height=64.

Примеры:
  cargo run --bin scan_code128_synth --
  cargo run --bin scan_code128_synth -- --text 0123456789 --set C
  cargo run --bin scan_code128_synth -- --text ABC --set A
"#
    );
}

// Простая запись PGM (P5, 8-бит)
fn write_pgm_p5(path: &str, width: usize, height: usize, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    write!(f, "P5\n{} {}\n255\n", width, height)?;
    f.write_all(data)?;
    Ok(())
}
