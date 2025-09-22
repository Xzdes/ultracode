//! Извлечение битов данных QR v1 (21×21) из матрицы модулей.

/// Возвращает 208 бит (26 кодвордов × 8) в порядке стандарта (правый нижний угол, двумя колонками, «змейкой»).
pub fn extract_data_bits_v1(grid: &[bool]) -> Vec<bool> {
    assert_eq!(grid.len(), 21*21, "ожидается матрица 21×21 (row-major)");
    let mut out = Vec::with_capacity(208);

    for (x, y) in walk_pairs_v1() {
        if is_function_v1(x, y) { continue; }
        let bit = grid[y * 21 + x];
        out.push(bit);
        if out.len() == 208 { break; }
    }
    out
}

/// Итератор по координатам (x,y) в порядке чтения данных QR v1:
/// начинаем справа-внизу, идём колонками парами, сверху-вниз/снизу-вверх, пропуская x=6 (timing col).
pub fn walk_pairs_v1() -> impl Iterator<Item = (usize, usize)> {
    struct It { x: isize, up: bool, phase: u8, y: isize }
    impl Iterator for It {
        type Item = (usize, usize);
        fn next(&mut self) -> Option<Self::Item> {
            loop {
                if self.x < 0 { return None; }
                if self.x == 6 { self.x -= 1; continue; } // пропускаем timing column

                // Идём по текущей паре колонок: сначала правая (x), затем левая (x-1) на каждой строке
                // self.phase: 0 — взять (x,y), 1 — взять (x-1,y), 2 — сдвинуть y и вернуть к phase=0
                match self.phase {
                    0 => {
                        // Координата (x, y)
                        let (xx, yy) = (self.x as usize, self.y as usize);
                        self.phase = 1;
                        return Some((xx, yy));
                    }
                    1 => {
                        let (xx, yy) = ((self.x - 1) as usize, self.y as usize);
                        self.phase = 2;
                        return Some((xx, yy));
                    }
                    _ => {
                        // двигаем y в зависимости от направления
                        if self.up {
                            if self.y > 0 { self.y -= 1; self.phase = 0; }
                            else {
                                // упёрлись в верх — перейдём в следующий блок слева
                                self.up = false;
                                self.x -= 2;
                                if self.x == 6 { self.x -= 1; } // перепрыгнуть timing
                                if self.x < 0 { return None; }
                                self.phase = 0;
                            }
                        } else {
                            if self.y < 20 { self.y += 1; self.phase = 0; }
                            else {
                                // упёрлись в низ — следующий блок слева и направление вверх
                                self.up = true;
                                self.x -= 2;
                                if self.x == 6 { self.x -= 1; }
                                if self.x < 0 { return None; }
                                self.phase = 0;
                            }
                        }
                    }
                }
            }
        }
    }

    It { x: 20, up: true, phase: 0, y: 20 }
}

/// Служебные модули QR v1 (которые нельзя читать как данные).
/// Включает: finder'ы + их белые сепараторы, timing row/col, оба набора format-инфо, dark module.
pub fn is_function_v1(x: usize, y: usize) -> bool {
    // Finder + separator области:
    if x <= 7 && y <= 7 { return true; }            // TL (0..7,0..7)
    if x >= 13 && y <= 7 { return true; }           // TR (13..20,0..7)
    if x <= 7 && y >= 13 { return true; }           // BL (0..7,13..20)

    // Timing
    if x == 6 || y == 6 { return true; }

    // Format information (две копии) вокруг TL/TR/BL
    if y == 8 && (x <= 8 || x >= 13) { return true; } // горизонтальные сегменты
    if x == 8 && (y <= 8 || y >= 13) { return true; } // вертикальные сегменты

    // Dark module (row = 4*ver+9 = 13, col = 8)
    if x == 8 && y == 13 { return true; }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_pairs_basic_properties() {
        // Должны обойти максимум 21*21 позиций (но мы пропускаем x=6).
        let v: Vec<_> = walk_pairs_v1().collect();
        assert!(v.len() <= 21*21);
        // Первая позиция — (20,20), т.к. начинаем справа-внизу
        assert_eq!(v[0], (20,20));
        // Где-то внутри должны встретить (0,0)
        assert!(v.contains(&(0,0)));
    }

    #[test]
    fn function_mask_counts_v1() {
        // Для v1 всего данных модулей = 26*8 = 208, значит служебных = 441-208 = 233.
        let mut func = 0usize;
        for y in 0..21 {
            for x in 0..21 {
                if is_function_v1(x,y) { func += 1; }
            }
        }
        assert_eq!(func, 441 - 208, "число служебных модулей должно сходиться");
    }
}
