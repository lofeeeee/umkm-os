//! Payment QRIS statis + refund (P3-7, ADR-0004). Murni: status payment, aturan konfirmasi,
//! batas refund. Penguncian baris & audit ada di infrastruktur.

use crate::catalog::Money;

pub const REFERENCE_MAX: usize = 120;
pub const NOTE_MAX: usize = 500;
pub const REASON_MAX: usize = 500;
/// Klaim bayar memperpanjang batas order agar staf sempat memverifikasi.
pub const CLAIM_HOLD_SECS: i64 = 48 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentStatus {
    Pending,
    Claimed,
    Confirmed,
    Rejected,
    Expired,
}

impl PaymentStatus {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "claimed" => Some(Self::Claimed),
            "confirmed" => Some(Self::Confirmed),
            "rejected" => Some(Self::Rejected),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }

    /// Pembeli boleh klaim/unggah bukti selama belum diputus.
    pub fn open(self) -> bool {
        matches!(self, Self::Pending | Self::Claimed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PaymentError {
    #[error("pembayaran sudah diputus")]
    AlreadyDecided,
    #[error("jumlah diterima berbeda dari tagihan: catatan wajib")]
    NoteRequiredForMismatch,
    #[error("jumlah refund melebihi sisa yang dapat dikembalikan")]
    RefundExceedsBalance,
    #[error("jumlah tidak valid")]
    InvalidAmount,
}

/// Konfirmasi: `received` > 0; berbeda dari tagihan → catatan wajib (diaudit).
pub fn check_confirm(
    status: PaymentStatus,
    billed: Money,
    received: Money,
    note: Option<&str>,
) -> Result<(), PaymentError> {
    if !status.open() {
        return Err(PaymentError::AlreadyDecided);
    }
    if received.sen() == 0 {
        return Err(PaymentError::InvalidAmount);
    }
    if received != billed && note.is_none_or(|n| n.trim().is_empty()) {
        return Err(PaymentError::NoteRequiredForMismatch);
    }
    Ok(())
}

/// Refund ≤ diterima − refund sebelumnya. `Ok(true)` bila refund ini melunasi seluruhnya.
pub fn check_refund(
    received_sen: i64,
    refunded_sen: i64,
    amount: Money,
) -> Result<bool, PaymentError> {
    if amount.sen() == 0 {
        return Err(PaymentError::InvalidAmount);
    }
    let left = received_sen - refunded_sen;
    if amount.sen() > left {
        return Err(PaymentError::RefundExceedsBalance);
    }
    Ok(amount.sen() == left)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Money {
        Money::parse(s).unwrap()
    }

    #[test]
    fn confirm_rules() {
        use PaymentStatus::*;
        assert!(check_confirm(Pending, m("100"), m("100"), None).is_ok());
        assert!(check_confirm(Claimed, m("100"), m("100"), None).is_ok());
        assert_eq!(
            check_confirm(Confirmed, m("100"), m("100"), None),
            Err(PaymentError::AlreadyDecided)
        );
        assert_eq!(
            check_confirm(Expired, m("100"), m("100"), None),
            Err(PaymentError::AlreadyDecided)
        );
        assert_eq!(
            check_confirm(Pending, m("100"), m("90"), None),
            Err(PaymentError::NoteRequiredForMismatch)
        );
        assert_eq!(
            check_confirm(Pending, m("100"), m("90"), Some(" ")),
            Err(PaymentError::NoteRequiredForMismatch)
        );
        assert!(
            check_confirm(Pending, m("100"), m("90"), Some("Kurang 10, dibayar tunai")).is_ok()
        );
        assert_eq!(
            check_confirm(Pending, m("100"), m("0"), Some("x")),
            Err(PaymentError::InvalidAmount)
        );
    }

    #[test]
    fn refund_balance() {
        assert_eq!(check_refund(10_000, 0, m("40")), Ok(false));
        assert_eq!(check_refund(10_000, 4_000, m("60")), Ok(true));
        assert_eq!(
            check_refund(10_000, 4_000, m("60.01")),
            Err(PaymentError::RefundExceedsBalance)
        );
        assert_eq!(
            check_refund(10_000, 0, m("0")),
            Err(PaymentError::InvalidAmount)
        );
    }
}
