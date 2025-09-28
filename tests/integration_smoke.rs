// tests/integration_smoke.rs
//
// Интеграционные «дымовые» тесты верхнего уровня.
// Здесь фиксируем, что Pipeline/Builder компилируются и базовые пути работают.
// Тесты с реальными изображениями помечены #[ignore] — снимем, когда положим файлы.

use std::{fs, io::{self, Read}};
use ultracode::{api::PipelineBuilder, prelude::*};

// ----------  helper module  ----------
mod util {
    use super::*;

    pub fn load_pgm_as_luma(path: &str) -> io::Result<LumaImage> {
        let mut file = fs::File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut i = 0;

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
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: no magic"))?;
        if magic != "P5" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "PGM: must be P5"));
        }
        let width: usize = read_token(&buf, &mut i)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: no width"))?
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PGM: bad width"))?;
        let height: usize = read_token(&buf, &mut i)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: no height"))?
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PGM: bad height"))?;
        let maxval: usize = read_token(&buf, &mut i)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PGM: no maxval"))?
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PGM: bad maxval"))?;
        if maxval != 255 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PGM: must be maxval=255",
            ));
        }

        if i < buf.len() && buf[i] == b'\n' {
            i += 1;
        }

        let data = buf[i..].to_vec();
        if data.len() != width * height {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PGM: data size mismatch",
            ));
        }

        Ok(LumaImage { data, width, height })
    }
}
// ----------  end helper  ----------

use util::load_pgm_as_luma;

fn ensure_test_qr_exists() {
    let dir = "tests/assets";
    let path = "tests/assets/qr_v1_l_hello.pgm";
    if fs::metadata(path).is_ok() {
        return;
    }
    fs::create_dir_all(dir).expect("Could not create tests/assets directory");

    // synthesise the QR code
    let gray_img = ultracode::qr::synthesize_qr_v1_from_text("HELLO", 3, 4);

    // write it as PGM
    use std::io::Write;
    let mut f = fs::File::create(path).expect("Could not create test PGM file");
    writeln!(f, "P5\n{} {}\n255", gray_img.width, gray_img.height).unwrap();
    f.write_all(gray_img.data).unwrap();
}

#[test]
fn pipeline_builder_compiles_and_runs_minimal() {
    let img = LumaImage {
        data: vec![255; 32 * 32],
        width: 32,
        height: 32,
    };

    let pipe = PipelineBuilder::new()
        .enable_ean13_upca(true)
        .enable_code128(true)
        .enable_qr(true)
        .build();

    let res = pipe.decode_all(&img);
    assert!(res.is_empty());
}

#[test]
fn pipeline_disable_qr_still_allows_1d() {
    let img = LumaImage {
        data: vec![0; 64 * 64],
        width: 64,
        height: 64,
    };

    let pipe = PipelineBuilder::new().enable_qr(false).build();
    let _ = pipe.decode_all(&img);
}

#[test]
fn decode_real_qr_v1_l_from_pgm() {
    ensure_test_qr_exists();

    let img = load_pgm_as_luma("tests/assets/qr_v1_l_hello.pgm")
        .expect("Failed to load test PGM image");

    let pipe = PipelineBuilder::new().build();
    let results = pipe.decode_all(&img);

    assert!(!results.is_empty(), "No QR code was decoded.");

    let qr_result = results.iter().find(|s| s.symbology == Symbology::QR);
    assert!(qr_result.is_some(), "No symbol with QR symbology found.");

    assert_eq!(qr_result.unwrap().text, "HELLO");
}

#[ignore]
#[test]
fn decode_real_qr_v1_m_from_png() {
    // placeholder – will be enabled when a real file is added
}