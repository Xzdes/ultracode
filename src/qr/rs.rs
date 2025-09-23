// src/qr/rs.rs
//! Reed–Solomon для QR (GF(256), примитивный полином 0x11D).
//! Полностью безопасная реализация без `unsafe` и без таблиц лог/эксп.
//! Есть кодирование (EC-байты) и ПОЛНАЯ коррекция ошибок для одного блока:
//! синдромы → Берлекэмп–Мэсси → поиск Чиена → Формула Форни.

/// Примитивный полином: x^8 + x^4 + x^3 + x^2 + 1
const GF_PRIM: u16 = 0x11D;
/// Генератор α (стандартно 2 для QR)
const GF_GEN: u8 = 2;

#[inline]
fn gf_add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Умножение в GF(256) «русским способом» с редукцией по 0x11D.
#[inline]
fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut res: u8 = 0;
    let mut aa = a as u16;
    let mut bb = b as u16;
    while bb != 0 {
        if (bb & 1) != 0 {
            res ^= aa as u8;
        }
        // умножаем a на x: сдвиг << 1, при переполнении редукция по примитивному полиному
        let carry = (aa & 0x80) != 0;
        aa = (aa << 1) & 0xFF;
        if carry {
            aa ^= GF_PRIM;
        }
        bb >>= 1;
    }
    res
}

/// Быстрое возведение в степень: a^e в GF(256).
#[inline]
fn gf_pow(mut a: u8, mut e: i32) -> u8 {
    if e == 0 {
        return 1;
    }
    if a == 0 {
        return 0;
    }
    // приведение показателя по модулю 255 (порядок мультипликативной группы)
    e %= 255;
    if e < 0 {
        e += 255;
    }
    let mut base = a;
    let mut exp = e as u32;
    let mut acc: u8 = 1;
    while exp > 0 {
        if (exp & 1) != 0 {
            acc = gf_mul(acc, base);
        }
        base = gf_mul(base, base);
        exp >>= 1;
    }
    acc
}

/// Обратный элемент: a^(−1) = a^254
#[inline]
fn gf_inv(a: u8) -> u8 {
    debug_assert!(a != 0);
    gf_pow(a, 254)
}

/// Вернуть `ec_len` байт контрольной суммы для `data` по QR-генератору.
/// Это «систематический» код: результат обычно конкатенируют к данным.
pub fn rs_ec_bytes(data: &[u8], ec_len: usize) -> Vec<u8> {
    let gen = generator_poly(ec_len);
    let mut rem = vec![0u8; ec_len];
    for &d in data {
        let coef = gf_add(d, rem[0]);
        // сдвиг остатков влево
        for i in 0..ec_len.saturating_sub(1) {
            rem[i] = rem[i + 1];
        }
        if ec_len > 0 {
            rem[ec_len - 1] = 0;
        }
        if coef != 0 {
            for (i, &g) in gen.iter().enumerate() {
                rem[i] = gf_add(rem[i], gf_mul(coef, g));
            }
        }
    }
    rem
}

/// Попытаться ИСПРАВИТЬ ошибки в одном RS-блоке длиной `data_len + ec_len`.
/// На вход подаём *весь* блок кодвордов (данные + защитные).
/// Возвращает Ok(количество_исправленных_байт), если удалось; Err(()) — если нет.
pub fn rs_correct_codeword_block(codewords: &mut [u8], data_len: usize, ec_len: usize) -> Result<usize, ()> {
    let n = data_len + ec_len;
    if codewords.len() != n || ec_len == 0 {
        return Err(());
    }

    // 1) Синдромы S_0..S_{ec_len-1}
    let synd = compute_syndromes(codewords, ec_len);
    if synd.iter().all(|&s| s == 0) {
        return Ok(0); // ошибок нет
    }

    // 2) Полиномы σ(x) и ω(x) по Берлекэмпу–Мэсси
    let (sigma, omega) = berlekamp_massey(&synd);

    // 3) Поиск корней σ(x) методом Чиена → позиции ошибок
    let err_pos = chien_search(&sigma, n);
    if err_pos.is_empty() {
        return Err(());
    }

    // 4) Величины ошибок по Формуле Форни и исправление
    let errs = err_pos.len();
    if errs > ec_len {
        return Err(()); // слишком много ошибок
    }

    let mut corrected = 0usize;
    for &pos in &err_pos {
        // x = α^(255 - pos) (позиционная нотация)
        let x = gf_pow(GF_GEN, (255 - pos as i32) % 255);
        let err_mag = forney_error_magnitude(&omega, &sigma, x);
        let idx = n - 1 - pos; // позиция от конца
        let before = codewords[idx];
        codewords[idx] = gf_add(codewords[idx], err_mag);
        if codewords[idx] != before {
            corrected += 1;
        }
    }

    // 5) Повторно проверим синдромы — должны занулиться.
    let post = compute_syndromes(codewords, ec_len);
    if post.iter().any(|&s| s != 0) {
        return Err(());
    }

    Ok(corrected)
}

// ---------------- внутренние функции RS ----------------

fn generator_poly(ec_len: usize) -> Vec<u8> {
    // g(x) = ∏_{i=0..ec_len-1} (x - α^{i})
    let mut g = vec![1u8];
    for i in 0..ec_len {
        let a = gf_pow(GF_GEN, i as i32);
        let mut ng = vec![0u8; g.len() + 1];
        for (j, &gj) in g.iter().enumerate() {
            // умножение на (x - a) = (x + a) в GF(2)
            // коэффициент при x^j
            ng[j] = gf_add(ng[j], gf_mul(gj, a));
            // коэффициент при x^{j+1}
            ng[j + 1] = gf_add(ng[j + 1], gj);
        }
        g = ng;
    }
    g
}

fn compute_syndromes(codewords: &[u8], ec_len: usize) -> Vec<u8> {
    // S_k = C(α^k), k = 0..ec_len-1
    let n = codewords.len();
    let mut synd = vec![0u8; ec_len];
    for k in 0..ec_len {
        let a_k = gf_pow(GF_GEN, k as i32);
        let mut acc = 0u8;
        for i in 0..n {
            // C(x) = ∑ c_i x^{n-1-i}
            let pow = gf_pow(a_k, (n - 1 - i) as i32);
            acc = gf_add(acc, gf_mul(codewords[i], pow));
        }
        synd[k] = acc;
    }
    synd
}

fn poly_scale(p: &[u8], s: u8) -> Vec<u8> {
    if s == 0 { return vec![0]; }
    p.iter().map(|&c| gf_mul(c, s)).collect()
}

fn poly_add(a: &[u8], b: &[u8]) -> Vec<u8> {
    let n = a.len().max(b.len());
    let mut out = vec![0u8; n];
    for i in 0..n {
        let ai = if i + a.len() >= n { a[i + a.len() - n] } else { 0 };
        let bi = if i + b.len() >= n { b[i + b.len() - n] } else { 0 };
        out[i] = gf_add(ai, bi);
    }
    trim_leading_zeros(&mut out);
    out
}

fn poly_mul(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; a.len() + b.len() - 1];
    for (i, &ai) in a.iter().enumerate() {
        if ai == 0 { continue; }
        for (j, &bj) in b.iter().enumerate() {
            if bj == 0 { continue; }
            out[i + j] = gf_add(out[i + j], gf_mul(ai, bj));
        }
    }
    trim_leading_zeros(&mut out);
    out
}

fn poly_derivative(p: &[u8]) -> Vec<u8> {
    // В GF(2^8) производная оставляет коэффициенты при нечётных степенях.
    if p.len() <= 1 { return vec![0]; }
    let mut out = Vec::with_capacity(p.len() - 1);
    let deg = p.len() - 1;
    for i in 0..deg {
        let coef = p[i];
        let power = deg - i;
        if power % 2 == 1 {
            out.push(coef);
        }
    }
    trim_leading_zeros(&mut out);
    out
}

fn trim_leading_zeros(v: &mut Vec<u8>) {
    while v.len() > 1 && v[0] == 0 {
        v.remove(0);
    }
}

fn berlekamp_massey(synd: &[u8]) -> (Vec<u8>, Vec<u8>) {
    // Возвращает (σ(x), ω(x)).
    let mut c = vec![1u8];
    let mut b = vec![1u8];
    let mut t;
    let mut l = 0i32;
    let mut m = 1i32;

    for n in 0..synd.len() {
        // рассогласование δ = S_n + sum_{i=1..L} c_i * S_{n-i}
        let mut delta = synd[n];
        for i in 1..=l as usize {
            delta = gf_add(delta, gf_mul(c[i], synd[n - i]));
        }

        if delta != 0 {
            t = c.clone();
            let mult = poly_scale(&b, delta);
            // c(x) = c(x) + x^m * mult
            let mut x_m_mult = vec![0u8; m as usize];
            x_m_mult.extend_from_slice(&mult);
            c = poly_add(&c, &x_m_mult);
            if 2 * l as usize <= n {
                l = (n + 1 - l as usize) as i32;
                b = poly_scale(&t, gf_inv(delta));
                m = 1;
            } else {
                m += 1;
            }
        } else {
            m += 1;
        }
    }

    // ω(x) = c(x) * S(x) mod x^{L}
    // S(x) = S_0 + S_1 x + ... + S_{t-1} x^{t-1}
    let s_poly = synd.to_vec();
    let mut omega = poly_mul(&c, &s_poly);
    if omega.len() > l as usize {
        omega = omega[omega.len() - l as usize..].to_vec();
    }
    trim_leading_zeros(&mut omega);

    (c, omega)
}

fn chien_search(sigma: &[u8], n: usize) -> Vec<usize> {
    // Ищем позиции j, где σ(α^{-j}) = 0. Позиции считаем 0..n-1 от конца.
    let mut err_pos = Vec::new();
    for j in 0..n {
        let x_inv = gf_pow(GF_GEN, (j as i32) % 255); // α^{j}
        let x = gf_inv(x_inv);                        // α^{-j}
        if poly_eval(sigma, x) == 0 {
            err_pos.push(j);
        }
    }
    err_pos
}

fn poly_eval(p: &[u8], x: u8) -> u8 {
    let mut y = 0u8;
    for &coef in p {
        y = gf_mul(y, x);
        y = gf_add(y, coef);
    }
    y
}

fn forney_error_magnitude(omega: &[u8], sigma: &[u8], x: u8) -> u8 {
    // e = - Ω(x^{-1}) / σ'(x^{-1})  (знак «-» = тот же XOR в GF(2^8))
    let x_inv = gf_inv(x);
    let num = poly_eval(omega, x_inv);
    let den_poly = poly_derivative(sigma);
    let den = poly_eval(&den_poly, x_inv);
    if den == 0 {
        return 0; // невосстановимо
    }
    gf_mul(num, gf_inv(den))
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
        // Вариант v1-L: 19 data + 7 ec = 26 cw
        let mut cw = vec![0u8; 26];
        // сгенерируем валидный блок: data произвольные, ec по функции
        for i in 0..19 {
            cw[i] = i as u8 ^ 0xA5;
        }
        let ec = rs_ec_bytes(&cw[..19], 7);
        cw[19..].copy_from_slice(&ec);

        // Вносим одиночную ошибку
        cw[3] ^= 0x5A;

        let mut work = cw.clone();
        let r = rs_correct_codeword_block(&mut work[..], 19, 7);
        assert!(r.is_ok());
        // Синдромы после коррекции должны быть нулевыми
        let synd = compute_syndromes(&work, 7);
        assert!(synd.iter().all(|&s| s == 0));
    }
}
