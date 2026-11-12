//! Metering & kuota (P5-8, `PRD.md §4.10` token/cost budget, ADR-0011). Murni.

use super::llm::Usage;

/// Estimasi token bila provider tidak melaporkan usage (≈ 4 karakter per token, dibulatkan ke atas).
pub fn estimate_tokens(chars: usize) -> u32 {
    chars.div_ceil(4).min(u32::MAX as usize) as u32
}

/// Usage terlapor, atau estimasi dari panjang prompt + jawaban.
pub fn effective_usage(reported: Option<Usage>, prompt_chars: usize, reply_chars: usize) -> Usage {
    reported.unwrap_or(Usage {
        prompt_tokens: estimate_tokens(prompt_chars),
        completion_tokens: estimate_tokens(reply_chars),
    })
}

/// Batas bulanan yang berlaku: platform = kuota operator (wajib); BYOK = budget tenant (opsional).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit(pub Option<u64>);

impl Limit {
    /// Boleh memulai giliran baru bila pemakaian bulan ini masih di bawah batas.
    /// Satu giliran boleh melewati sisa kuota (tidak dipotong di tengah jawaban); giliran berikutnya ditolak.
    pub fn allows(self, used_this_month: u64) -> bool {
        self.0.is_none_or(|l| used_this_month < l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_and_limits() {
        assert_eq!(estimate_tokens(0), 0);
        assert_eq!(estimate_tokens(9), 3);
        let u = effective_usage(None, 400, 41);
        assert_eq!((u.prompt_tokens, u.completion_tokens), (100, 11));
        let r = Usage {
            prompt_tokens: 5,
            completion_tokens: 6,
        };
        assert_eq!(effective_usage(Some(r), 400, 400), r);
        assert!(Limit(None).allows(u64::MAX));
        assert!(Limit(Some(1000)).allows(999));
        assert!(!Limit(Some(1000)).allows(1000));
    }
}
