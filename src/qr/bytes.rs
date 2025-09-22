//! Биты ↔ байты и разбор Byte mode для QR v1-L.

/// Упаковать 208 бит (MSB-first) в 26 байт (codewords).
pub fn bits_to_bytes_v1(bits: &[bool]) -> Vec<u8> {
    let mut out = Vec::with_capacity((bits.len()+7)/8);
    let mut cur: u8 = 0;
    let mut k = 0;
    for &b in bits {
        cur = (cur << 1) | if b {1} else {0};
        k += 1;
        if k == 8 {
            out.push(cur);
            cur = 0; k = 0;
        }
    }
    if k > 0 {
        out.push(cur << (8 - k)); // добьём нулями в MSB-порядке
    }
    out
}

/// Разобрать Byte mode для v1-L (19 data bytes, 7 ec) **напрямую из битового потока**.
/// Считаем первые 19×8=152 бита данных (остальное — EC), порядок бит MSB-first.
/// Формат: 4 бита mode=0100, 8 бит length, затем `length` байтов данных.
pub fn parse_byte_mode_bits_v1_l(bits: &[bool]) -> Option<String> {
    let data_bits = 19 * 8;
    if bits.len() < data_bits { return None; }
    parse_byte_mode_bits_v1_l_from_offset(bits, 0)
}

/// «Умный» relaxed-парсер: сканирует заголовок `0100` с произвольного смещения
/// в пределах первых 19 байт данных. Полезно, если поток оказался сдвинут.
pub fn parse_byte_mode_bits_v1_l_relaxed(bits: &[bool]) -> Option<String> {
    let data_bits = 19 * 8;
    if bits.len() < data_bits { return None; }
    let s = &bits[..data_bits];

    // Можно сканировать с шагом 1 бита. Чтобы не ловить ложные срабатывания,
    // проверяем: len<=17, хватает бит до конца, payload валиден как UTF-8.
    for offset in 0..=(data_bits.saturating_sub(12)) {
        if let Some(txt) = parse_byte_mode_bits_v1_l_from_offset(s, offset) {
            return Some(txt);
        }
    }
    None
}

fn parse_byte_mode_bits_v1_l_from_offset(bits: &[bool], offset: usize) -> Option<String> {
    let data_bits = 19 * 8;
    if bits.len() < data_bits || offset + 12 > data_bits { return None; }

    struct R<'a> { b: &'a [bool], i: usize }
    impl<'a> R<'a> {
        fn new(b: &'a [bool], start: usize) -> Self { Self { b, i: start } }
        fn left(&self) -> usize { self.b.len().saturating_sub(self.i) }
        fn get(&mut self, n: usize) -> Option<u32> {
            if self.i + n > self.b.len() { return None; }
            let mut v = 0u32;
            for _ in 0..n {
                v = (v << 1) | (self.b[self.i] as u32);
                self.i += 1;
            }
            Some(v)
        }
    }

    let mut r = R::new(&bits[..data_bits], offset);
    let mode = r.get(4)? as u8;
    if mode != 0b0100 { return None; }
    let len = r.get(8)? as usize;
    if len > 17 { return None; }
    if r.left() < len * 8 { return None; }

    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
        let b = r.get(8)? as u8;
        bytes.push(b);
    }
    String::from_utf8(bytes).ok()
}

/// Старый парсер по codewords — оставляем для API и тестов совместимости.
/// Разобрать Byte mode из первых 19 data codewords.
pub fn parse_byte_mode_v1_l(data_cw: &[u8]) -> Option<String> {
    if data_cw.len() < 1 { return None; }

    // Битридер: MSB-first по codewords.
    struct R<'a> { cw: &'a [u8], i: usize, b: u8 }
    impl<'a> R<'a> {
        fn new(cw: &'a [u8]) -> Self { Self { cw, i: 0, b: 0 } }
        fn get(&mut self, n: usize) -> Option<u32> {
            let mut v = 0u32;
            for _ in 0..n {
                if self.i/8 >= self.cw.len() { return None; }
                if self.i % 8 == 0 { self.b = self.cw[self.i/8]; }
                let bit = (self.b & (1 << (7 - (self.i%8)))) != 0;
                v = (v << 1) | (bit as u32);
                self.i += 1;
            }
            Some(v)
        }
    }

    let mut r = R::new(data_cw);
    let mode = r.get(4)? as u8;
    if mode != 0b0100 { return None; }
    let len = r.get(8)? as usize;
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
        bytes.push(r.get(8)? as u8);
    }
    String::from_utf8(bytes).ok()
}
