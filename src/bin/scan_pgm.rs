use std::env;
use std::fs;
use std::io;

use ultracode::{decode_any, DecodeOptions, GrayImage};

fn main() -> io::Result<()> {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: scan_pgm <file.pgm>");
            return Ok(());
        }
    };

    let bytes = fs::read(&path)?;
    let (width, height, data) = parse_pgm_p5(&bytes).unwrap_or_else(|e| {
        eprintln!("PGM parse error: {e}");
        std::process::exit(2);
    });

    // GrayImage заимствует срез данных — держим Vec в области видимости.
    let img_buf = data;
    let img = GrayImage {
        width,
        height,
        data: &img_buf,
    };

    let opts = DecodeOptions::default();
    let decoded = decode_any(img, opts);

    if decoded.is_empty() {
        println!("(no symbols)");
    } else {
        for b in decoded {
            println!(
                "{:?}: {}  (conf={:.2}, orient={:?})",
                b.symbology,
                b.text,
                b.confidence,
                b.orientation
            );
        }
    }

    Ok(())
}

/// Простой парсер бинарного PGM (P5, maxval=255, поддержка комментариев).
fn parse_pgm_p5(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    if bytes.len() < 2 || &bytes[..2] != b"P5" {
        return Err("not a P5 pgm".into());
    }
    let mut i = 2usize;

    // пропускаем пробелы/переводы строк/комментарии
    let skip_ws = |idx: &mut usize| {
        loop {
            // пробелы
            while *idx < bytes.len()
                && matches!(bytes[*idx], b' ' | b'\n' | b'\r' | b'\t')
            {
                *idx += 1;
            }
            // комментарии
            if *idx < bytes.len() && bytes[*idx] == b'#' {
                while *idx < bytes.len() && bytes[*idx] != b'\n' {
                    *idx += 1;
                }
                continue;
            }
            break;
        }
    };

    let take_while = |idx: &mut usize, pred: fn(u8) -> bool| {
        let start = *idx;
        while *idx < bytes.len() && pred(bytes[*idx]) {
            *idx += 1;
        }
        &bytes[start..*idx]
    };

    skip_ws(&mut i);
    let w_str = take_while(&mut i, |c| c.is_ascii_digit());
    skip_ws(&mut i);
    let h_str = take_while(&mut i, |c| c.is_ascii_digit());
    skip_ws(&mut i);
    let mv_str = take_while(&mut i, |c| c.is_ascii_digit());
    // один байт разделителя после maxval
    if i < bytes.len() && matches!(bytes[i], b'\n' | b'\r' | b' ') {
        i += 1;
    }

    let width = std::str::from_utf8(w_str)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or("bad width")?;
    let height = std::str::from_utf8(h_str)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or("bad height")?;
    let maxval = std::str::from_utf8(mv_str)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or("bad maxval")?;
    if maxval != 255 {
        return Err("only maxval=255 supported".into());
    }

    let need = width.checked_mul(height).ok_or("size overflow")?;
    if bytes.len() < i + need {
        return Err("truncated pixel data".into());
    }
    let data = bytes[i..i + need].to_vec();
    Ok((width, height, data))
}
