use ultracode::{decode_any, DecodeOptions, GrayImage};

fn main() {
    // Простой синтетический пустой кадр (как smoke-тест сборки).
    let width = 128usize;
    let height = 128usize;
    let buf = vec![255u8; width * height];

    let img = GrayImage {
        width,
        height,
        data: &buf,
    };

    let opts = DecodeOptions::default();
    let out = decode_any(img, opts);

    if out.is_empty() {
        println!("(no symbols)");
    } else {
        for b in out {
            println!(
                "{:?}: {}  (conf={:.2}, orient={:?})",
                b.symbology,
                b.text,
                b.confidence,
                b.orientation
            );
        }
    }
}
