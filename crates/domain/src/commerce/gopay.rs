//! Pencocokan pembayaran masuk GoPay Merchant ke pesanan (P10, integrasi gateway). Murni: tanpa IO.
//!
//! Sumber data: transaksi dari merchant-analytics GoBiz. Pesanan dicocokkan dari **nominal unik**
//! (lihat [`super::qris::with_unique`]) di dalam jendela waktu, lalu didedup per `transaction_id`
//! agar satu transaksi tidak diklaim dua pesanan (anti double-claim seperti di gateway asal).

/// Status transaksi dari GoBiz (`transaction_status`, huruf kecil).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnStatus {
    Settlement,
    Capture,
    Refund,
    PartialRefund,
    Other,
}

impl TxnStatus {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "settlement" => Self::Settlement,
            "capture" => Self::Capture,
            "refund" => Self::Refund,
            "partial_refund" => Self::PartialRefund,
            _ => Self::Other,
        }
    }

    /// Hanya settlement/capture yang dianggap uang benar-benar masuk.
    pub fn is_paid(self) -> bool {
        matches!(self, Self::Settlement | Self::Capture)
    }
}

/// Transaksi masuk yang sudah dinormalisasi (nominal rupiah bulat, waktu epoch-ms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GopayTxn {
    pub id: String,
    pub amount_rupiah: u64,
    pub status: TxnStatus,
    pub time_ms: i64,
    pub issuer: String,
    pub order_id: Option<String>,
}

/// Cari transaksi yang melunasi tagihan: nominal sama persis, sudah settle, waktu ≥ `since_ms`, dan
/// belum diklaim. `is_claimed` memberi tahu id transaksi mana yang sudah dipakai pesanan lain.
///
/// Mengembalikan transaksi pertama yang cocok (urutan input dipertahankan; gateway mengirim terbaru
/// dulu). Caller lalu menandai `txn.id` sebagai terklaim untuk pesanan ini.
pub fn match_payment(
    target_rupiah: u64,
    since_ms: i64,
    txns: &[GopayTxn],
    is_claimed: impl Fn(&str) -> bool,
) -> Option<&GopayTxn> {
    txns.iter().find(|t| {
        t.status.is_paid()
            && t.amount_rupiah == target_rupiah
            && t.time_ms >= since_ms
            && !is_claimed(&t.id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txn(id: &str, amount: u64, status: &str, time_ms: i64) -> GopayTxn {
        GopayTxn {
            id: id.into(),
            amount_rupiah: amount,
            status: TxnStatus::parse(status),
            time_ms,
            issuer: "GoPay".into(),
            order_id: None,
        }
    }

    #[test]
    fn status_paid_only_settlement_capture() {
        assert!(TxnStatus::parse("SETTLEMENT").is_paid());
        assert!(TxnStatus::parse("capture").is_paid());
        assert!(!TxnStatus::parse("refund").is_paid());
        assert!(!TxnStatus::parse("pending").is_paid());
    }

    #[test]
    fn matches_exact_amount_in_window() {
        let txns = [
            txn("t1", 30_000, "settlement", 1_000), // nominal beda (tanpa kode unik)
            txn("t2", 30_217, "settlement", 2_000), // cocok
        ];
        let m = match_payment(30_217, 1_500, &txns, |_| false).unwrap();
        assert_eq!(m.id, "t2");
    }

    #[test]
    fn ignores_too_old_unsettled_and_claimed() {
        let txns = [
            txn("old", 30_217, "settlement", 500),       // sebelum jendela
            txn("pend", 30_217, "pending", 3_000),       // belum settle
            txn("claimed", 30_217, "settlement", 3_100), // sudah diklaim
            txn("ok", 30_217, "capture", 3_200),         // cocok
        ];
        let m = match_payment(30_217, 1_000, &txns, |id| id == "claimed").unwrap();
        assert_eq!(m.id, "ok");
        // Tanpa transaksi valid → None.
        assert!(match_payment(99_999, 1_000, &txns, |_| false).is_none());
    }
}
