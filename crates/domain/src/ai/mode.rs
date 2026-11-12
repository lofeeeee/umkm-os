//! Mode chatbot (P5-9, `PRD.md §4.10`, FR/AC-AIC-004). Matriks keputusan murni:
//! apa yang terjadi pada jawaban AI menurut mode tenant.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMode {
    /// AI dapat mengirim jawaban sesuai policy.
    Auto,
    /// AI membuat draft, admin mengirim.
    Assisted,
    /// AI menangani pertanyaan tertentu, sisanya eskalasi. Default yang disarankan.
    #[default]
    Hybrid,
    /// AI tidak pernah mengirim jawaban (dan provider tidak dipanggil).
    HumanOnly,
}

pub const ALL_MODES: [AiMode; 4] = [
    AiMode::Auto,
    AiMode::Assisted,
    AiMode::Hybrid,
    AiMode::HumanOnly,
];

impl AiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Assisted => "assisted",
            Self::Hybrid => "hybrid",
            Self::HumanOnly => "human_only",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        ALL_MODES.into_iter().find(|m| m.as_str() == s)
    }
    /// Human Only: pipeline berhenti sebelum provider (AC-AIC-004).
    pub fn calls_provider(self) -> bool {
        self != Self::HumanOnly
    }
}

/// Hasil pipeline AI sebelum keputusan pengiriman.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Jawaban lolos validasi, berdasar konteks, dalam topik yang diizinkan.
    Grounded,
    /// Jawaban ada tetapi tidak cukup yakin / di luar topik yang diizinkan / tanpa sumber.
    Uncertain,
    /// Pelanggan meminta manusia, atau topik terlarang.
    NeedsHuman,
    /// Guardrail menolak input/output, provider gagal/timeout, atau kuota habis.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Kirim jawaban AI ke pelanggan.
    SendAuto,
    /// Simpan sebagai draft untuk admin.
    DraftForAdmin,
    /// Kirim pesan fallback tenant + alihkan ke manusia.
    Handover,
    /// Tidak ada balasan AI; percakapan menunggu admin.
    HumanQueue,
}

pub fn decide(mode: AiMode, outcome: Outcome) -> Decision {
    use {AiMode::*, Decision::*, Outcome::*};
    match (mode, outcome) {
        (HumanOnly, _) => HumanQueue,
        (_, NeedsHuman | Failed) => Handover,
        (Assisted, _) => DraftForAdmin,
        (Auto, Grounded | Uncertain) => SendAuto,
        (Hybrid, Grounded) => SendAuto,
        (Hybrid, Uncertain) => Handover,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use {AiMode::*, Decision::*, Outcome::*};

    /// Matriks 4 mode × 4 hasil, ditulis lengkap (P5-9).
    #[test]
    fn mode_matrix() {
        let want = [
            (Auto, [SendAuto, SendAuto, Handover, Handover]),
            (Assisted, [DraftForAdmin, DraftForAdmin, Handover, Handover]),
            (Hybrid, [SendAuto, Handover, Handover, Handover]),
            (HumanOnly, [HumanQueue, HumanQueue, HumanQueue, HumanQueue]),
        ];
        for (mode, row) in want {
            for (o, d) in [Grounded, Uncertain, NeedsHuman, Failed]
                .into_iter()
                .zip(row)
            {
                assert_eq!(decide(mode, o), d, "{mode:?} {o:?}");
            }
        }
    }

    #[test]
    fn human_only_never_sends_nor_calls_provider() {
        for o in [Grounded, Uncertain, NeedsHuman, Failed] {
            assert!(!matches!(decide(HumanOnly, o), SendAuto | DraftForAdmin));
        }
        assert!(!HumanOnly.calls_provider() && Hybrid.calls_provider());
        assert_eq!(AiMode::default(), Hybrid);
        for m in ALL_MODES {
            assert_eq!(AiMode::parse(m.as_str()), Some(m));
        }
    }
}
