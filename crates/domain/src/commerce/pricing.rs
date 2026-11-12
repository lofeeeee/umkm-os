//! Perhitungan harga server (FR-ORD-002, AC-ORD-002). Klien tidak pernah menjadi sumber
//! harga: input hanya `(variant_id, quantity)`; harga diambil dari katalog yang dikunci.

use uuid::Uuid;

use crate::catalog::{Money, format_sen};

/// Maksimum baris & kuantitas per baris satu order.
pub const MAX_LINES: usize = 50;
pub const MAX_QTY_PER_LINE: i64 = 999;

/// Baris cart dari klien (tanpa harga).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartLine {
    pub variant_id: Uuid,
    pub quantity: i64,
}

/// Snapshot katalog saat checkout (dibaca server dalam transaksi).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogItem {
    pub variant_id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub variant_name: String,
    pub sku: String,
    pub unit_price: Money,
    /// Produk active + available, varian aktif.
    pub sellable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricedLine {
    pub item: CatalogItem,
    pub quantity: i64,
    pub line_total_sen: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub lines: Vec<PricedLine>,
    pub subtotal_sen: i64,
    pub delivery_fee_sen: i64,
    pub total_sen: i64,
}

impl Quote {
    pub fn total(&self) -> String {
        format_sen(self.total_sen)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CartError {
    #[error("cart kosong")]
    Empty,
    #[error("terlalu banyak baris")]
    TooManyLines,
    #[error("kuantitas tidak valid pada baris {0}")]
    InvalidQuantity(usize),
    #[error("produk tidak tersedia pada baris {0}")]
    Unavailable(usize),
    #[error("total melebihi batas")]
    Overflow,
}

/// Gabungkan baris varian sama (urutan kemunculan pertama dipertahankan) lalu validasi.
pub fn normalize_cart(lines: &[CartLine]) -> Result<Vec<CartLine>, CartError> {
    if lines.is_empty() {
        return Err(CartError::Empty);
    }
    if lines.len() > MAX_LINES {
        return Err(CartError::TooManyLines);
    }
    let mut out: Vec<CartLine> = vec![];
    for (i, l) in lines.iter().enumerate() {
        if !(1..=MAX_QTY_PER_LINE).contains(&l.quantity) {
            return Err(CartError::InvalidQuantity(i));
        }
        match out.iter_mut().find(|x| x.variant_id == l.variant_id) {
            Some(x) => {
                x.quantity += l.quantity;
                if x.quantity > MAX_QTY_PER_LINE {
                    return Err(CartError::InvalidQuantity(i));
                }
            }
            None => out.push(*l),
        }
    }
    Ok(out)
}

/// Hitung total dari harga katalog. `catalog` = item yang ditemukan untuk tenant ini;
/// varian yang tidak ada/tidak dapat dijual → `Unavailable` (AC-ORD-001).
pub fn quote(
    cart: &[CartLine],
    catalog: &[CatalogItem],
    delivery_fee: Money,
) -> Result<Quote, CartError> {
    let cart = normalize_cart(cart)?;
    let mut lines = vec![];
    let mut subtotal: i64 = 0;
    for (i, l) in cart.iter().enumerate() {
        let item = catalog
            .iter()
            .find(|c| c.variant_id == l.variant_id && c.sellable)
            .ok_or(CartError::Unavailable(i))?;
        let line_total = item
            .unit_price
            .checked_mul(l.quantity)
            .ok_or(CartError::Overflow)?;
        subtotal = subtotal
            .checked_add(line_total)
            .ok_or(CartError::Overflow)?;
        lines.push(PricedLine {
            item: item.clone(),
            quantity: l.quantity,
            line_total_sen: line_total,
        });
    }
    let total = subtotal
        .checked_add(delivery_fee.sen())
        .ok_or(CartError::Overflow)?;
    // NUMERIC(14,2) → maks 999.999.999.999,99
    if total >= 100_000_000_000_000 {
        return Err(CartError::Overflow);
    }
    Ok(Quote {
        lines,
        subtotal_sen: subtotal,
        delivery_fee_sen: delivery_fee.sen(),
        total_sen: total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: u128, price: &str, sellable: bool) -> CatalogItem {
        CatalogItem {
            variant_id: Uuid::from_u128(n),
            product_id: Uuid::from_u128(100 + n),
            product_name: format!("P{n}"),
            variant_name: "Default".into(),
            sku: format!("S{n}"),
            unit_price: Money::parse(price).unwrap(),
            sellable,
        }
    }

    fn line(n: u128, q: i64) -> CartLine {
        CartLine {
            variant_id: Uuid::from_u128(n),
            quantity: q,
        }
    }

    #[test]
    fn totals_from_catalog_only() {
        let cat = [item(1, "15000", true), item(2, "2500.50", true)];
        let q = quote(
            &[line(1, 2), line(2, 1), line(1, 1)],
            &cat,
            Money::parse("10000").unwrap(),
        )
        .unwrap();
        assert_eq!(q.lines.len(), 2, "baris varian sama digabung");
        assert_eq!(q.lines[0].quantity, 3);
        assert_eq!(q.subtotal_sen, 4_500_000 + 250_050);
        assert_eq!(q.total(), "57500.50");
    }

    #[test]
    fn invalid_carts() {
        // AC-ORD-001
        let cat = [item(1, "1", true), item(2, "1", false)];
        let z = Money::ZERO;
        assert_eq!(quote(&[], &cat, z), Err(CartError::Empty));
        assert_eq!(
            quote(&[line(1, 0)], &cat, z),
            Err(CartError::InvalidQuantity(0))
        );
        assert_eq!(
            quote(&[line(1, -1)], &cat, z),
            Err(CartError::InvalidQuantity(0))
        );
        assert_eq!(
            quote(&[line(1, 1), line(2, 1)], &cat, z),
            Err(CartError::Unavailable(1))
        );
        assert_eq!(
            quote(&[line(9, 1)], &cat, z),
            Err(CartError::Unavailable(0))
        );
        assert_eq!(
            quote(&[line(1, 600), line(1, 600)], &cat, z),
            Err(CartError::InvalidQuantity(1))
        );
        let many: Vec<_> = (0..51).map(|i| line(i, 1)).collect();
        assert_eq!(quote(&many, &cat, z), Err(CartError::TooManyLines));
    }
}
