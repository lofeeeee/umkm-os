//! Inventory (P3-2, `PRD.md §4.5`, `PRD.md §11.1`). Murni: aturan level stok, reservasi,
//! policy stok negatif, dan deteksi low-stock. Penguncian baris + ledger ada di infrastruktur.

/// Batas kuantitas per mutasi / reservasi.
pub const MAX_QTY: i64 = 1_000_000;
pub const REASON_MAX: usize = 200;
pub const REFERENCE_MAX: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub on_hand: i64,
    pub reserved: i64,
    /// AC-INV-004: default `false` → stok tersedia tidak boleh < 0.
    pub allow_negative: bool,
    pub min_stock: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StockError {
    #[error("stok tidak cukup (tersedia {available}, diminta {requested})")]
    Insufficient { available: i64, requested: i64 },
    #[error("kuantitas tidak valid")]
    InvalidQuantity,
    #[error("reservasi melebihi jumlah yang dipesan")]
    ReservedUnderflow,
}

fn qty_ok(q: i64) -> Result<(), StockError> {
    if (1..=MAX_QTY).contains(&q) {
        Ok(())
    } else {
        Err(StockError::InvalidQuantity)
    }
}

impl Level {
    /// Stok yang masih bisa dijual.
    pub fn available(self) -> i64 {
        self.on_hand - self.reserved
    }

    fn checked(self, requested: i64) -> Result<Self, StockError> {
        if !self.allow_negative && self.available() < 0 {
            return Err(StockError::Insufficient {
                available: self.available() + requested,
                requested,
            });
        }
        Ok(self)
    }

    /// Stock in (`delta > 0`) / out (`delta < 0`).
    pub fn apply(self, delta: i64) -> Result<Self, StockError> {
        if delta == 0 || delta.abs() > MAX_QTY {
            return Err(StockError::InvalidQuantity);
        }
        Self {
            on_hand: self.on_hand + delta,
            ..self
        }
        .checked((-delta).max(0))
    }

    /// Stock opname: set jumlah fisik. Delta 0 ditolak (tidak ada movement kosong).
    pub fn set_count(self, counted: i64) -> Result<(Self, i64), StockError> {
        if !(0..=MAX_QTY * 100).contains(&counted) {
            return Err(StockError::InvalidQuantity);
        }
        let delta = counted - self.on_hand;
        Ok((self.apply(delta)?, delta))
    }

    /// Reservasi checkout (AC-INV-003/005).
    pub fn reserve(self, qty: i64) -> Result<Self, StockError> {
        qty_ok(qty)?;
        Self {
            reserved: self.reserved + qty,
            ..self
        }
        .checked(qty)
    }

    /// Reservasi dilepas (batal/kedaluwarsa): stok tersedia kembali.
    pub fn release(self, qty: i64) -> Result<Self, StockError> {
        qty_ok(qty)?;
        if self.reserved < qty {
            return Err(StockError::ReservedUnderflow);
        }
        Ok(Self {
            reserved: self.reserved - qty,
            ..self
        })
    }

    /// Reservasi menjadi penjualan: stok fisik dan reservasi sama-sama turun.
    pub fn commit(self, qty: i64) -> Result<Self, StockError> {
        qty_ok(qty)?;
        if self.reserved < qty {
            return Err(StockError::ReservedUnderflow);
        }
        Ok(Self {
            on_hand: self.on_hand - qty,
            reserved: self.reserved - qty,
            ..self
        })
    }

    pub fn is_low(self) -> bool {
        self.min_stock.is_some_and(|m| self.available() < m)
    }
}

/// AC-INV-002: event low-stock hanya saat melintasi ambang (bukan setiap mutasi di bawahnya).
pub fn crossed_low(before: Level, after: Level) -> bool {
    !before.is_low() && after.is_low()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementKind {
    StockIn,
    StockOut,
    Adjustment,
    Sale,
}

impl MovementKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StockIn => "stock_in",
            Self::StockOut => "stock_out",
            Self::Adjustment => "adjustment",
            Self::Sale => "sale",
        }
    }

    /// Jenis yang boleh dibuat manual lewat API (sale hanya dari order).
    pub fn parse_manual(s: &str) -> Option<Self> {
        match s {
            "stock_in" => Some(Self::StockIn),
            "stock_out" => Some(Self::StockOut),
            "adjustment" => Some(Self::Adjustment),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(on_hand: i64, reserved: i64) -> Level {
        Level {
            on_hand,
            reserved,
            allow_negative: false,
            min_stock: Some(5),
        }
    }

    #[test]
    fn negative_stock_rejected_by_default() {
        // AC-INV-004
        assert_eq!(
            lv(3, 0).apply(-4),
            Err(StockError::Insufficient {
                available: 3,
                requested: 4
            })
        );
        assert_eq!(
            lv(3, 2).apply(-2).unwrap_err(),
            StockError::Insufficient {
                available: 1,
                requested: 2
            }
        );
        let neg = Level {
            allow_negative: true,
            ..lv(3, 0)
        };
        assert_eq!(neg.apply(-4).unwrap().on_hand, -1);
        assert_eq!(lv(3, 0).apply(0), Err(StockError::InvalidQuantity));
    }

    #[test]
    fn reserve_release_commit() {
        let l = lv(10, 0).reserve(4).unwrap();
        assert_eq!((l.on_hand, l.reserved, l.available()), (10, 4, 6));
        assert!(l.reserve(7).is_err());
        let l = l.release(4).unwrap();
        assert_eq!(
            l.available(),
            10,
            "AC-INV-003: reservasi dilepas -> tersedia kembali"
        );
        let l = l.reserve(3).unwrap().commit(3).unwrap();
        assert_eq!((l.on_hand, l.reserved), (7, 0));
        assert_eq!(l.commit(1), Err(StockError::ReservedUnderflow));
        assert_eq!(l.release(1), Err(StockError::ReservedUnderflow));
        assert_eq!(l.reserve(0), Err(StockError::InvalidQuantity));
    }

    #[test]
    fn count_and_low_stock_crossing() {
        let (l, d) = lv(10, 0).set_count(4).unwrap();
        assert_eq!((l.on_hand, d), (4, -6));
        assert!(crossed_low(lv(10, 0), l));
        assert!(
            !crossed_low(l, l.apply(-1).unwrap()),
            "sudah di bawah ambang: tidak ada event baru"
        );
        assert!(
            crossed_low(lv(6, 0), lv(6, 2)),
            "reservasi juga bisa melintasi ambang"
        );
        let none = Level {
            min_stock: None,
            ..lv(0, 0)
        };
        assert!(!none.is_low());
        assert!(lv(4, 0).set_count(-1).is_err());
    }
}
