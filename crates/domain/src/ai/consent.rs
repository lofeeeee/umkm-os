//! Pemberitahuan & persetujuan AI untuk pelanggan (D-12 bagian AI, DISETUJUI bersyarat 2026-11-12).
//! PLACEHOLDER: wajib diganti teks final hasil review legal sebelum go-live. P6 menampilkan teks ini di
//! widget chat dan menyimpan `VERSION` per percakapan bersama waktu persetujuan.

pub const VERSION: &str = "ai-consent-placeholder-v0";

pub const TEXT: &str = "Anda sedang berbicara dengan asisten AI toko ini. Jawaban dibuat otomatis dan dapat \
keliru; untuk hal penting, minta dihubungkan dengan staf kapan saja dengan menulis \"bicara dengan admin\". \
Pesan Anda disimpan toko untuk melayani Anda dan dihapus mengikuti kebijakan retensi. Jangan kirim kata sandi, \
PIN, atau data kartu pembayaran di chat.";

/// Elemen minimum yang harus ada di teks final juga (dipakai uji agar teks pengganti tetap memenuhi).
pub const REQUIRED_ELEMENTS: [&str; 4] = [
    "asisten AI",
    "keliru",
    "bicara dengan admin",
    "Jangan kirim",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_states_ai_limits_human_option_and_data_warning() {
        for e in REQUIRED_ELEMENTS {
            assert!(TEXT.contains(e), "{e}");
        }
        assert!(
            VERSION.contains("placeholder"),
            "ganti versi saat teks final"
        );
        assert!(TEXT.chars().count() <= 500);
    }
}
