# ultracode

Zero-deps, high-performance barcode engine in Rust.  
**Now:** EAN-13 / UPC-A via scanlines.  
**Next:** QR (finder + warp + RS), Code128, Code39.

## Why raw grayscale?
Avoids image parsing dependencies and lets you feed frames directly from camera or your own loader.  
`u8` buffer is row-major, 1 byte per pixel.

## Usage

```rust
use ultracode::{GrayImage, decode_any, DecodeOptions};

let width = 640usize;
let height = 480usize;
let frame: Vec<u8> = vec![128; width*height]; // your grayscale data

let img = GrayImage { width, height, data: &frame };
let opts = DecodeOptions::default();
let results = decode_any(img, opts);

for b in results {
    println!("{:?}: {}", b.format, b.text);
}
````

## Performance tips

* Pre-crop ROI around expected barcode area.
* Increase `scan_rows` only if нужно (default 15).
* Feed good contrast (auto-exposure/auto-gain).
* For real-time video, scan только соседние кадры и кэшируй последние успешные результаты.

## License

MIT OR Apache-2.0

```

---

### что дальше (план расширения под «все форматы» и «молниеносно»)

1) **Скорость без зависимостей**
   - обработка только над `&[u8]`, без аллокаций в горячем пути;
   - предвычислять пороги на несколько линий разом; реюзить буферы `Vec<bool>` и `Vec<usize>`;
   - добавить SIMD (nightly) через `core::arch` для сумм/минимов по строкам (опционально, фича-флаг).

2) **Поддержка форматов**
   - 1D: Code128 (таблица 107 паттернов), Code39, Interleaved 2 of 5.
   - 2D: QR (v1-v5 сначала), потом DataMatrix (L-shape finder проще, но нужен Reed–Solomon GF(256)).

3) **QR реализация**
   - поиск паттернов через скан-линии (как сейчас для 1D) + проверка соотношений 1:1:3:1:1;
   - кластеризация центров (без зависимостей: k-means «вручную» или RANSAC-подобная оценка тройки);
   - warp в сетку через обратную проекцию 3×3 (solve homography по трём углам — закрытая форма);
   - декодер RS: реализовать GF(256) с x^8 + x^4 + x^3 + x^2 + 1, таблицы log/exp.

если хочешь — в следующем шаге могу дописать быстрый детектор Code128 или первый «сканер» QR (finder patterns). скажи, в каком направлении продолжать, и я пришлю **полные файлы** так же, как выше.
```
