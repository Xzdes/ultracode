// src/qr/rs.rs
//! Reed–Solomon для QR (GF(256), примитивный полином 0x11D).
//! Полностью safe-реализация без таблиц.
//!
//! Соглашения:
//! - Внутренние многочлены храним в массиве по ВОЗРАСТАНИЮ степени (p[i] == coef(x^i)).
//! - Массив кодвордов `codewords` — high-degree-first (индекс 0 — старшая степень).
//! - Для синдромов: S_k = C(α^k), C(x)=∑ c_i x^{n-1-i}.

const GF_PRIM: u16 = 0x11D;              // x^8 + x^4 + x^3 + x^2 + 1
const GF_REDUCE8: u16 = GF_PRIM ^ 0x100; // 0x1D — редукция по младшим 8 битам
const GF_GEN: u8 = 2;                    // α

#[inline] fn gf_add(a: u8, b: u8) -> u8 { a ^ b }

#[inline]
fn gf_mul(a: u8, b: u8) -> u8 {
    let mut aa = a as u16;
    let mut bb = b as u16;
    let mut r: u8 = 0;
    while bb != 0 {
        if (bb & 1) != 0 { r ^= aa as u8; }
        let carry = (aa & 0x80) != 0;
        aa = (aa << 1) & 0xFF;
        if carry { aa ^= GF_REDUCE8; }
        bb >>= 1;
    }
    r
}

#[inline]
fn gf_pow(a: u8, mut e: i32) -> u8 {
    if e == 0 { return 1; }
    if a == 0 { return 0; }
    e %= 255;
    if e < 0 { e += 255; }
    let mut base = a;
    let mut acc: u8 = 1;
    let mut exp = e as u32;
    while exp > 0 {
        if (exp & 1) != 0 { acc = gf_mul(acc, base); }
        base = gf_mul(base, base);
        exp >>= 1;
    }
    acc
}

#[inline] fn gf_inv(a: u8) -> u8 { debug_assert!(a != 0); gf_pow(a, 254) }

// ---------------- poly helpers (ascending-degree representation) ----------------

#[inline]
fn trim_high_zeros(v: &mut Vec<u8>) {
    while v.len() > 1 && *v.last().unwrap() == 0 { v.pop(); }
}

#[inline]
fn poly_add(a: &[u8], b: &[u8]) -> Vec<u8> {
    let n = a.len().max(b.len());
    let mut out = vec![0u8; n];
    for i in 0..n {
        let ai = if i < a.len() { a[i] } else { 0 };
        let bi = if i < b.len() { b[i] } else { 0 };
        out[i] = gf_add(ai, bi);
    }
    trim_high_zeros(&mut out);
    out
}

#[inline]
fn poly_scale(p: &[u8], s: u8) -> Vec<u8> {
    if s == 0 { return vec![0]; }
    let mut out: Vec<u8> = p.iter().map(|&c| gf_mul(c, s)).collect();
    trim_high_zeros(&mut out);
    out
}

#[inline]
fn poly_mul(a: &[u8], b: &[u8]) -> Vec<u8> {
    if a.len() == 1 && a[0] == 0 { return vec![0]; }
    if b.len() == 1 && b[0] == 0 { return vec![0]; }
    let mut out = vec![0u8; a.len() + b.len() - 1];
    for (i, &ai) in a.iter().enumerate() {
        if ai == 0 { continue; }
        for (j, &bj) in b.iter().enumerate() {
            if bj == 0 { continue; }
            out[i + j] = gf_add(out[i + j], gf_mul(ai, bj));
        }
    }
    trim_high_zeros(&mut out);
    out
}

#[inline]
fn poly_shift_x(p: &[u8], m: usize) -> Vec<u8> {
    if p.len() == 1 && p[0] == 0 { return vec![0]; }
    let mut out = vec![0u8; m];
    out.extend_from_slice(p);
    out
}

#[inline]
fn poly_eval(p: &[u8], x: u8) -> u8 {
    // Horner по ВОЗРАСТАНИЮ степени: идём от старшей к младшей (с конца к началу).
    let mut y = 0u8;
    for &coef in p.iter().rev() {
        y = gf_mul(y, x);
        y = gf_add(y, coef);
    }
    y
}

#[inline]
fn poly_derivative(p: &[u8]) -> Vec<u8> {
    // d/dx x^k = k*x^{k-1}; в GF(2) остаются только нечётные k.
    if p.len() <= 1 { return vec![0]; }
    let mut out = Vec::with_capacity(p.len() - 1);
    for k in 1..p.len() {
        if (k & 1) == 1 { out.push(p[k]); } else { out.push(0); }
    }
    trim_high_zeros(&mut out);
    out
}

// ---------------- generator (narrow-sense) ----------------

/// Генераторный полином степени `ec_len` (корни α^1..α^{ec_len}).
/// Возвращаем ровно `ec_len` младших коэффициентов (без старшей 1 при x^{ec_len}).
fn generator_poly(ec_len: usize) -> Vec<u8> {
    let mut g = vec![1u8]; // степень 0
    for i in 0..ec_len {
        let a = gf_pow(GF_GEN, (i as i32) + 1);
        // (x - a) = a + x в GF(2)
        let factor = vec![a, 1u8]; // a + x
        g = poly_mul(&g, &factor);
    }
    // g длиной ec_len+1, отбрасываем старшую 1
    let mut res = g;
    res.pop();
    trim_high_zeros(&mut res);
    res // длина == ec_len, коэффициенты при x^0..x^{ec_len-1}
}

// ---------------- public API ----------------

/// ECC для `data` (систематический RS). Возвращаем блок длиной `ec_len`,
/// который просто дописывается в конец `data` (high-degree-first порядок).
pub fn rs_ec_bytes(data: &[u8], ec_len: usize) -> Vec<u8> {
    if ec_len == 0 { return vec![]; }

    // g_full(x) = g0 + ... + g_{ec-1} x^{ec-1} + 1·x^{ec}  (ascending)
    let mut g_full = generator_poly(ec_len);
    g_full.push(1);

    // Переводим в «старшая→младшая» для прямого деления:
    let mut g_rev = g_full.clone();
    g_rev.reverse();

    // M(x)·x^{ec} в high-degree-first: data + ec нулей справа.
    let mut rem: Vec<u8> = Vec::with_capacity(data.len() + ec_len);
    rem.extend_from_slice(data);
    rem.resize(data.len() + ec_len, 0);

    // Деление слева→вправо.
    for i in 0..data.len() {
        let coef = rem[i];
        if coef != 0 {
            for j in 0..=ec_len {
                rem[i + j] = gf_add(rem[i + j], gf_mul(coef, g_rev[j]));
            }
        }
    }

    // Остаток — последние ec_len байт.
    rem[data.len()..data.len() + ec_len].to_vec()
}

/// Исправить ошибки в одном RS-блоке длиной `data_len + ec_len`.
pub fn rs_correct_codeword_block(codewords: &mut [u8], data_len: usize, ec_len: usize) -> Result<usize, ()> {
    let n = data_len + ec_len;
    if codewords.len() != n || ec_len == 0 { return Err(()); }

    // 1) синдромы
    let synd = compute_syndromes(codewords, ec_len);
    if synd.iter().all(|&s| s == 0) { return Ok(0); }

    // 2) Берлекэмп–Мэсси → σ, ω
    let (sigma, omega) = berlekamp_massey(&synd);

    // 3) Чиен: ищем ПРАВЫЕ индексы i (0 — правый край), где σ(α^{-i}) = 0.
    //    Для записи в массив байт используем левый индекс j = n-1-i.
    let err_pos = chien_search_right_index(&sigma, n);
    if err_pos.is_empty() || err_pos.len() > ec_len { return Err(()); }

    // 4) Форни: X = α^{i}, используем X^{-1}; правый i → левый j = n-1-i.
    let sigma_der = poly_derivative(&sigma);
    let mut corrected = 0usize;
    for &i_right in &err_pos {
        let j_left = n - 1 - i_right;
        let x = gf_pow(GF_GEN, i_right as i32);
        let x_inv = gf_inv(x);
        let num = poly_eval(&omega, x_inv);
        let den = poly_eval(&sigma_der, x_inv);
        if den == 0 { return Err(()); }
        let e = gf_mul(num, gf_inv(den));
        let before = codewords[j_left];
        codewords[j_left] = gf_add(codewords[j_left], e);
        if codewords[j_left] != before { corrected += 1; }
    }

    // 5) проверка
    let post = compute_syndromes(codewords, ec_len);
    if post.iter().any(|&s| s != 0) { return Err(()); }

    Ok(corrected)
}

// ---------------- internal: syndromes, BM, Chien ----------------

/// Синдромы S_k = C(α^k), k=1..=ec_len. C(x)=∑ c_i x^{n-1-i}.
fn compute_syndromes(codewords: &[u8], ec_len: usize) -> Vec<u8> {
    let n = codewords.len();
    let mut synd = vec![0u8; ec_len];
    for k in 1..=ec_len {
        let a_k = gf_pow(GF_GEN, k as i32);
        let mut acc = 0u8;
        for i in 0..n {
            let pow = gf_pow(a_k, (n - 1 - i) as i32);
            acc = gf_add(acc, gf_mul(codewords[i], pow));
        }
        synd[k - 1] = acc;
    }
    synd
}

/// Берлекэмп–Мэсси. Возвращает (σ(x), ω(x)) в ascending-представлении.
/// σ[0] = 1. ω = (σ * S) mod x^L, где S = S1 + S2 x + ... (ascending).
fn berlekamp_massey(synd: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut sigma = vec![1u8];
    let mut b = vec![1u8];
    let mut l: usize = 0;
    let mut m: usize = 1;

    for n in 0..synd.len() {
        // δ = S_n + sum_{i=1..L} σ_i * S_{n-i}
        let mut delta = synd[n];
        for i in 1..=l {
            if i < sigma.len() {
                delta = gf_add(delta, gf_mul(sigma[i], synd[n - i]));
            }
        }
        if delta != 0 {
            let t = sigma.clone();
            let upd = poly_shift_x(&poly_scale(&b, delta), m);
            sigma = poly_add(&sigma, &upd);
            if 2 * l <= n {
                l = n + 1 - l;
                b = poly_scale(&t, gf_inv(delta));
                m = 1;
            } else {
                m += 1;
            }
        } else {
            m += 1;
        }
    }

    // ω(x) = (σ(x) * S(x)) mod x^L
    let s_poly = synd.to_vec(); // ascending: S1 + S2 x + ...
    let mut omega = poly_mul(&sigma, &s_poly);
    omega.truncate(l);          // mod x^L
    trim_high_zeros(&mut omega);

    (sigma, omega)
}

/// Чиен: возвращает ПРАВОсторонние индексы i (0 — правый край), где σ(α^{-i}) = 0.
/// Для массива байт потом берём j = n-1-i.
fn chien_search_right_index(sigma: &[u8], n: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for i in 0..n {
        let x_inv = gf_pow(GF_GEN, -(i as i32)); // α^{-i}
        if poly_eval(sigma, x_inv) == 0 {
            out.push(i);
        }
    }
    out
}

// ---------------- tests ----------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rs_simple_known() {
        let data = b"HELLO WORLD 123";
        let ec = rs_ec_bytes(data, 7);
        assert_eq!(ec.len(), 7);
    }

    #[test]
    fn corrects_single_error_in_v1_l_block() {
        // v1-L: 19 data + 7 ec
        let mut cw = vec![0u8; 26];
        for i in 0..19 { cw[i] = i as u8 ^ 0xA5; }
        let ec = rs_ec_bytes(&cw[..19], 7);
        cw[19..].copy_from_slice(&ec);

        // одиночная ошибка
        cw[3] ^= 0x5A;

        let mut work = cw.clone();
        let r = rs_correct_codeword_block(&mut work[..], 19, 7);
        assert!(r.is_ok(), "RS correction failed");
        let synd = compute_syndromes(&work, 7);
        assert!(synd.iter().all(|&s| s == 0), "syndromes not cleared");
    }
}
