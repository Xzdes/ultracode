//! Мини Reed–Solomon для QR (GF(256), poly 0x11D), для генерации ECC в синтетике.

#[inline] fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    // Быстрый «русский крестьянский» на поле 0x11D
    let mut res = 0u8;
    while b != 0 {
        if (b & 1) != 0 { res ^= a; }
        let hi = (a & 0x80) != 0;
        a <<= 1;
        if hi { a ^= 0x1D; } // неприв. полином без старшего бита (0x11D -> 0x1D)
        b >>= 1;
    }
    res
}

fn poly_mul(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; a.len() + b.len() - 1];
    for (i, &ai) in a.iter().enumerate() {
        if ai == 0 { continue; }
        for (j, &bj) in b.iter().enumerate() {
            out[i+j] ^= gf_mul(ai, bj);
        }
    }
    out
}

/// Сгенерировать коэффициенты порождающего полинома степени `ec_len`: ∏_{i=0}^{ec_len-1} (x - α^i).
fn rs_generator(ec_len: usize) -> Vec<u8> {
    let mut g = vec![1u8];
    let mut a_pow = 1u8; // α^0 = 1, α=2 (в нашей gf_mul модели это корректно)
    for _i in 0..ec_len {
        // (x - α^i) == x + α^i  (так как в GF(2^8) «минус» = «плюс»)
        g = poly_mul(&g, &[1u8, a_pow]);
        // следующее α^{i+1} = α * α^i == gf_mul(2, α^i)
        a_pow = gf_mul(2, a_pow);
    }
    g
}

/// Вычислить  `ec_len` байт из data codewords (QR: v1-L -> 7 EC).
pub fn rs_ec_bytes(data: &[u8], ec_len: usize) -> Vec<u8> {
    let g = rs_generator(ec_len);
    // Делим (data || 0^ec_len) на g(x), берём остаток.
    let mut msg = vec![0u8; data.len() + ec_len];
    msg[..data.len()].copy_from_slice(data);

    for i in 0..data.len() {
        let coef = msg[i];
        if coef == 0 { continue; }
        for j in 0..g.len() {
            msg[i + j] ^= gf_mul(coef, g[j]);
        }
    }
    msg[data.len()..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rs_simple_known() {
        // Короткая проверка на инвариант: если data=0.., то ec не нули.
        let data = [1u8,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19];
        let ec = rs_ec_bytes(&data, 7);
        assert_eq!(ec.len(), 7);
        assert!(ec.iter().any(|&b| b != 0));
    }
}
