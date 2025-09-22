use std::{
    env, fs,
    io::{self, Read},
};
use ultracode::{decode_any, DecodeOptions, GrayImage};

fn main() {
    let mut path: Option<String> = None;
    let mut scan_rows: Option<usize> = None;

    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--rows" => {
                if let Some(v) = args.next() {
                    scan_rows = Some(v.parse().unwrap_or(15));
                }
            }
            "--help" | "-h" => {
                print_help();
                return;
            }
            other => {
                if path.is_none() {
                    path = Some(other.to_string());
                } else {
                    eprintln!("Лишний аргумент: {other}");
                    print_help();
                    std::process::exit(2);
                }
            }
        }
    }

    let path = match path {
        Some(p) => p,
        None => {
            print_help();
            std::process::exit(2);
        }
    };

    let (width, height, data) = match read_pgm_p5(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Не удалось прочитать PGM: {e}");
            std::process::exit(1);
        }
    };

    let mut opts = DecodeOptions::default();
    if let Some(r) = scan_rows {
        opts.scan_rows = r;
    }
    let img = GrayImage {
        width,
        height,
        data: &data,
    };
    let results = decode_any(img, opts);

    if results.is_empty() {
        println!("Ничего не распознано.");
    } else {
        for b in results {
            println!("{:?}: {}  (row={})", b.format, b.text, b.row);
        }
    }
}

fn print_help() {
    eprintln!(
        r#"Использование:
  cargo run --bin scan_pgm -- <path.pgm> [--rows <N>]

Требуется PGM P5 (8-бит, maxval=255).
Примеры:
  cargo run --bin scan_pgm -- ./test.pgm
  cargo run --bin scan_pgm -- ./test.pgm --rows 25
"#
    );
}

// Минимальный парсер PGM (P5, 8-бит)
fn read_pgm_p5(path: &str) -> io::Result<(usize, usize, Vec<u8>)> {
    let mut file = fs::File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    // читаем ascii-хедер "P5\n<width> <height>\n<maxval>\n"
    let mut i = 0usize;

    fn read_token(buf: &[u8], i: &mut usize) -> Option<String> {
        while *i < buf.len() {
            let c = buf[*i];
            if c == b'#' {
                while *i < buf.len() && buf[*i] != b'\n' {
                    *i += 1;
                }
            } else if c.is_ascii_whitespace() {
                *i += 1;
            } else {
                break;
            }
        }
        if *i >= buf.len() {
            return None;
        }
        let start = *i;
        while *i < buf.len() && !buf[*i].is_ascii_whitespace() {
            *i += 1;
        }
        Some(String::from_utf8_lossy(&buf[start..*i]).to_string())
    }

    let magic = read_token(&buf, &mut i)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: нет магической сигнатуры"))?;
    if magic != "P5" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PGM: поддерживается только P5 (binary)",
        ));
    }
    let width: usize = read_token(&buf, &mut i)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: нет width"))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PGM: неверный width"))?;
    let height: usize = read_token(&buf, &mut i)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: нет height"))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PGM: неверный height"))?;
    let maxval: usize = read_token(&buf, &mut i)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: нет maxval"))?
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PGM: неверный maxval"))?;
    if maxval != 255 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PGM: поддерживается только maxval=255",
        ));
    }

    if i >= buf.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "PGM: нет данных изображения",
        ));
    }
    if buf[i] == b'\n' {
        i += 1;
    }
    let expected = width.checked_mul(height)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: переполнение размера"))?;
    if buf.len() - i < expected {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "PGM: мало байтов данных",
        ));
    }
    let data = buf[i..i + expected].to_vec();
    Ok((width, height, data))
}
