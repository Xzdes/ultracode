//! QR helpers (v1): QrOptions + чтение формат-путей.

pub mod bytes;
pub mod encode;
pub mod finder;
pub mod format;
pub mod rs;
pub mod sample;
pub mod data;

#[derive(Clone, Debug)]
pub struct QrOptions {
    /// Сколько строк сканировать при поиске кандидатов finder pattern.
    pub scan_lines: usize,
    /// Включить подробные отладочные логи.
    pub debug: bool,
}

impl Default for QrOptions {
    fn default() -> Self {
        Self {
            scan_lines: 64,
            debug: true,
        }
    }
}

/// Удобный реэкспорт для тестов/интеграции
pub use encode::synthesize_qr_v1_from_text;

use self::format::{decode_format_word, EcLevel, FORMAT_READ_PATHS_V1};

/// Чтение формат-информации из матрицы (булева 21×21).
pub fn decode_v1_format_from_matrix(matrix: &[Vec<bool>]) -> Option<(EcLevel, u8, u32, usize)> {
    let n = data::N1;
    if matrix.len() != n || matrix.iter().any(|r| r.len() != n) {
        return None;
    }

    #[derive(Clone, Debug)]
    struct FormatCandidate {
        ec: EcLevel,
        mask_id: u8,
        total_hamming_dist: u32,
        track: usize,
    }

    let mut tops: Vec<FormatCandidate> = Vec::with_capacity(FORMAT_READ_PATHS_V1.len());

    for (track_idx, path) in FORMAT_READ_PATHS_V1.iter().enumerate() {
        // Собираем 15 бит по дорожке
        let mut word: u32 = 0;
        for &(x, y) in path {
            word = (word << 1) | if matrix[y as usize][x as usize] { 1 } else { 0 };
        }

        // decode_format_word ожидает u16
        if let Some((ec, mask_id, hamming)) = decode_format_word(word as u16) {
            tops.push(FormatCandidate {
                ec,
                mask_id,
                total_hamming_dist: hamming,
                track: track_idx,
            });
        }
    }

    if tops.is_empty() {
        return None;
    }

    tops.sort_by_key(|c| c.total_hamming_dist);
    let best = &tops[0];

    println!(
        "[qr] format candidates: {} | best: ec={:?} mask={} hamming={} track={}",
        tops.len(),
        best.ec,
        best.mask_id,
        best.total_hamming_dist,
        best.track
    );

    Some((best.ec, best.mask_id, best.total_hamming_dist, best.track))
}
