//! Pemberitahuan & persetujuan AI untuk pelanggan (D-12). Teks v1 disusun sendiri oleh tim dan disetujui pemilik
//! produk 2026-11-13 (tanpa review konsultan hukum, product-decision-log). Widget chat menampilkan teks ini dan
//! menyimpan `VERSION` per percakapan bersama waktu persetujuan. Mengubah teks = naikkan `VERSION`.

pub const VERSION: &str = "ai-consent-v1-20261113";

pub const TEXT: &str = "Anda sedang berbicara dengan asisten AI toko ini. Jawaban dibuat otomatis dan dapat \
keliru; untuk hal penting, minta dihubungkan dengan staf kapan saja dengan menulis \"bicara dengan admin\". \
Pesan Anda dan informasi toko yang relevan diproses oleh penyedia AI untuk menjawab, disimpan toko untuk melayani \
Anda, dan dihapus 30 hari setelah pesan terakhir. Jangan kirim kata sandi, PIN, atau data kartu pembayaran di chat.";

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
            !VERSION.contains("placeholder") && VERSION.starts_with("ai-consent-v"),
            "teks final D-12 harus berversi"
        );
        assert!(TEXT.chars().count() <= 500);
    }
}
