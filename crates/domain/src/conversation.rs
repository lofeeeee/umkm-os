//! Conversation (P6-1, `PRD.md §4.12`, FR-CNV-001, ADR-0006). Murni: 4 sender type, lifecycle
//! status (dicerminkan trigger DB, paritas diuji), validasi isi pesan, aturan token sesi chat.

use std::time::Duration;

pub const BODY_MAX: usize = 4000;
/// Sesi chat anonim: berlaku sejak aktivitas terakhir (diperpanjang tiap pesan pelanggan).
pub const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

/// AC-CNV-001: tepat empat pengirim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SenderType {
    Customer,
    Ai,
    Admin,
    System,
}

pub const ALL_SENDERS: [SenderType; 4] = [
    SenderType::Customer,
    SenderType::Ai,
    SenderType::Admin,
    SenderType::System,
];

impl SenderType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Customer => "CUSTOMER",
            Self::Ai => "AI",
            Self::Admin => "ADMIN",
            Self::System => "SYSTEM",
        }
    }

    /// Persis (case-sensitive); nilai lain ditolak.
    pub fn parse(s: &str) -> Option<Self> {
        ALL_SENDERS.into_iter().find(|x| x.as_str() == s)
    }
}

/// Status percakapan.
/// - `Ai`: dijawab AI sesuai mode tenant.
/// - `WaitingHuman`: handover diminta / mode Human Only; menunggu staf.
/// - `Human`: diambil alih staf; AI berhenti membalas otomatis.
/// - `Closed`: final; pelanggan memulai percakapan baru.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConversationStatus {
    Ai,
    WaitingHuman,
    Human,
    Closed,
}

pub const ALL_STATUSES: [ConversationStatus; 4] = [
    ConversationStatus::Ai,
    ConversationStatus::WaitingHuman,
    ConversationStatus::Human,
    ConversationStatus::Closed,
];

impl ConversationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ai => "AI",
            Self::WaitingHuman => "WAITING_HUMAN",
            Self::Human => "HUMAN",
            Self::Closed => "CLOSED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        ALL_STATUSES.into_iter().find(|x| x.as_str() == s)
    }

    /// Status awal yang sah.
    pub fn is_initial(self) -> bool {
        matches!(self, Self::Ai | Self::WaitingHuman)
    }

    pub fn can_transition(self, to: Self) -> bool {
        use ConversationStatus::*;
        matches!(
            (self, to),
            (Ai, WaitingHuman | Human | Closed)
                | (WaitingHuman, Human | Ai | Closed)
                | (Human, Ai | WaitingHuman | Closed)
        )
    }

    /// AI hanya boleh membalas otomatis saat status `Ai` (takeover menghentikan auto-response).
    pub fn ai_may_reply(self) -> bool {
        self == Self::Ai
    }

    pub fn accepts_messages(self) -> bool {
        self != Self::Closed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyError {
    Empty,
    TooLong,
    ControlCharacter,
}

impl BodyError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Empty => "required",
            Self::TooLong => "too_long",
            Self::ControlCharacter => "control_character",
        }
    }
}

/// Isi pesan: dipangkas, 1..=4000 karakter, tanpa karakter kontrol selain baris baru/tab.
pub fn validate_body(raw: &str) -> Result<String, BodyError> {
    let normalized = raw.replace("\r\n", "\n");
    let s = normalized.trim();
    if s.is_empty() {
        return Err(BodyError::Empty);
    }
    if s.chars().count() > BODY_MAX {
        return Err(BodyError::TooLong);
    }
    if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(BodyError::ControlCharacter);
    }
    Ok(s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_four_sender_types() {
        let names: Vec<_> = ALL_SENDERS.iter().map(|s| s.as_str()).collect();
        assert_eq!(names, ["CUSTOMER", "AI", "ADMIN", "SYSTEM"]);
        for s in ALL_SENDERS {
            assert_eq!(SenderType::parse(s.as_str()), Some(s));
        }
        for bad in ["customer", "BOT", "STAFF", "", "AGENT", "ADMIN "] {
            assert_eq!(SenderType::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn lifecycle() {
        use ConversationStatus::*;
        assert!(Ai.is_initial() && WaitingHuman.is_initial());
        assert!(!Human.is_initial() && !Closed.is_initial());
        assert!(Ai.can_transition(Human));
        assert!(Human.can_transition(Ai), "staf mengembalikan ke AI");
        assert!(WaitingHuman.can_transition(Human));
        for s in ALL_STATUSES {
            assert!(!Closed.can_transition(s), "CLOSED final");
            assert!(
                !s.can_transition(s),
                "{s:?} -> dirinya sendiri bukan transisi"
            );
        }
        assert!(Ai.ai_may_reply());
        for s in [WaitingHuman, Human, Closed] {
            assert!(!s.ai_may_reply());
        }
        assert!(!Closed.accepts_messages());
        for s in ALL_STATUSES {
            assert_eq!(ConversationStatus::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn body_rules() {
        assert_eq!(validate_body("  halo \r\n kak "), Ok("halo \n kak".into()));
        assert_eq!(validate_body("   "), Err(BodyError::Empty));
        assert_eq!(
            validate_body(&"a".repeat(BODY_MAX)).map(|s| s.len()),
            Ok(BODY_MAX)
        );
        assert_eq!(
            validate_body(&"é".repeat(BODY_MAX + 1)),
            Err(BodyError::TooLong)
        );
        assert_eq!(validate_body("a\u{0}b"), Err(BodyError::ControlCharacter));
        assert_eq!(validate_body("a\tb\nc"), Ok("a\tb\nc".into()));
    }
}
