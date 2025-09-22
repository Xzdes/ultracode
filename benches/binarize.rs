use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ultracode::binarize::{binarize_row, binarize_row_adaptive, normalize_modules, runs};

fn make_row(width: usize, seed: u32) -> Vec<u8> {
    // Немного «полосатого» шума, чтобы бенч был стабильным и не совсем рандомным
    let mut x = seed;
    (0..width)
        .map(|i| {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = ((x >> 24) & 0xFF) as u8;
            if (i / 7) % 2 == 0 {
                v.saturating_add(32)
            } else {
                v.saturating_sub(32)
            }
        })
        .collect()
}

fn bench_binarize(c: &mut Criterion) {
    let width = 2048usize;
    let row = make_row(width, 123);

    c.bench_function("binarize_row", |b| {
        b.iter(|| {
            // Новый API: возвращает Vec<bool>
            let bin = binarize_row(black_box(&row));
            black_box(bin.len())
        })
    });

    c.bench_function("binarize_row_adaptive", |b| {
        b.iter(|| {
            // Тоже возвращает Vec<bool>
            let bin = binarize_row_adaptive(black_box(&row));
            black_box(bin.len())
        })
    });

    c.bench_function("runs + normalize_modules", |b| {
        b.iter(|| {
            // Полный конвейер: бинаризация -> ран-лены -> нормализация модулей
            let bin = binarize_row_adaptive(black_box(&row)); // &[u8] -> Vec<bool>
            let r = runs(&bin);                               // принимает &[bool]
            let _nm = normalize_modules(&bin, &r);            // принимает (&[bool], &runs)
            black_box(r.len())
        })
    });
}

criterion_group!(benches, bench_binarize);
criterion_main!(benches);
