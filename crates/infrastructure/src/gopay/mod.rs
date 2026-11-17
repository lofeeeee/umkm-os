//! Integrasi GoPay Merchant (GoBiz) — login OTP, refresh, ambil transaksi untuk konfirmasi
//! pembayaran QRIS otomatis (P10, ADR-0020). Lihat juga `umkm_domain::commerce::{qris, gopay}`.

pub mod client;
pub mod keepalive;
pub mod poller;
pub mod qrsvg;
pub mod session;
pub mod time;

pub use client::{
    GobizClient, GobizError, OtpChallenge, normalize_phone, parse_session_blob, parse_txns,
};
pub use keepalive::{KeepaliveStats, RefreshMode, RefreshOutcome, ensure_fresh, keepalive_due};
pub use poller::{PollStats, poll_due};
pub use session::{GopaySession, KEEPALIVE_MS, REFRESH_SKEW_MS, TOKEN_TTL_MS, TokenPair};
