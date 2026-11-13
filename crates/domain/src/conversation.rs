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

/// Masking data sensitif untuk pembaca tanpa hak menangani chat (FR-CNV-003, AC-CNV-003).
pub mod mask {
    /// Pengganti nilai yang disembunyikan seluruhnya.
    pub const HIDDEN: &str = "•••";
    /// Deret ≥ 8 digit (telepon, rekening, kartu, NIK) dianggap sensitif.
    pub const MIN_DIGITS: usize = 8;

    /// `budi@x.id` → `b•••@x.id`.
    pub fn email(s: &str) -> String {
        match s.split_once('@') {
            Some((local, domain)) if !local.is_empty() && !domain.is_empty() => {
                let first: String = local.chars().take(1).collect();
                format!("{first}{HIDDEN}@{domain}")
            }
            _ => HIDDEN.to_owned(),
        }
    }

    /// Hanya 3 digit terakhir yang terlihat.
    pub fn phone(s: &str) -> String {
        let digits: Vec<char> = s.chars().filter(char::is_ascii_digit).collect();
        if digits.len() <= 3 {
            return HIDDEN.to_owned();
        }
        let tail: String = digits[digits.len() - 3..].iter().collect();
        format!("{HIDDEN}{tail}")
    }

    /// `Budi Santoso` → `B••• S•••`.
    pub fn name(s: &str) -> String {
        let parts: Vec<String> = s
            .split_whitespace()
            .map(|w| {
                let first: String = w.chars().take(1).collect();
                format!("{first}{HIDDEN}")
            })
            .collect();
        if parts.is_empty() {
            HIDDEN.to_owned()
        } else {
            parts.join(" ")
        }
    }

    fn is_email_char(c: char) -> bool {
        c.is_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-' | '@')
    }

    /// Isi pesan: alamat email dan deret ≥ 8 digit (boleh dipisah spasi/`-`/`.`/`+`/kurung)
    /// disamarkan; teks lain utuh.
    pub fn text(s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            // Email: token [email-char]* yang memuat '@' dengan titik setelahnya.
            if is_email_char(c) && (i == 0 || !is_email_char(chars[i - 1])) {
                let mut j = i;
                while j < chars.len() && is_email_char(chars[j]) {
                    j += 1;
                }
                let tok: String = chars[i..j].iter().collect();
                let tok_trim = tok.trim_end_matches(['.', '-']);
                if let Some((l, d)) = tok_trim.split_once('@')
                    && !l.is_empty()
                    && d.contains('.')
                    && !d.starts_with('.')
                {
                    out.push_str(&email(tok_trim));
                    out.push_str(&tok[tok_trim.len()..]);
                    i = j;
                    continue;
                }
            }
            // Deret angka: digit + pemisah umum, dihitung digitnya.
            if c.is_ascii_digit()
                || (c == '+' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
            {
                let mut j = i;
                let mut digits = 0;
                let mut last_digit = i;
                while j < chars.len()
                    && (chars[j].is_ascii_digit()
                        || matches!(chars[j], ' ' | '-' | '.' | '+' | '(' | ')'))
                {
                    if chars[j].is_ascii_digit() {
                        digits += 1;
                        last_digit = j;
                    }
                    j += 1;
                }
                if digits >= MIN_DIGITS {
                    let run: String = chars[i..=last_digit].iter().collect();
                    out.push_str(&phone(&run));
                    i = last_digit + 1;
                    continue;
                }
                // Bukan deret sensitif: salin sampai digit terakhir saja.
                out.extend(&chars[i..=last_digit.max(i)]);
                i = last_digit.max(i) + 1;
                continue;
            }
            out.push(c);
            i += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking() {
        use super::mask::*;
        assert_eq!(email("budi@toko.id"), "b•••@toko.id");
        assert_eq!(email("rusak"), HIDDEN);
        assert_eq!(phone("+62 812-3456-7890"), "•••890");
        assert_eq!(name("Budi  Santoso"), "B••• S•••");
        assert_eq!(name(""), HIDDEN);
        let m = text(
            "Halo, saya budi.s@mail.co.id. WA 0812 3456 7890 ya, pesan 2 kopi jam 10.30 (Rp 85.000).",
        );
        assert_eq!(
            m,
            "Halo, saya b•••@mail.co.id. WA •••890 ya, pesan 2 kopi jam 10.30 (Rp 85.000)."
        );
        assert_eq!(text("kartu 4111-1111-1111-1111"), "kartu •••111");
        assert_eq!(text("NIK 3201234567890001!"), "NIK •••001!");
        // < 8 digit (kode pesanan) tetap; tanggal 8 digit ikut disamarkan (konservatif).
        assert_eq!(text("ORD-1234567 dan 2026-11-13"), "ORD-1234567 dan •••113");
        assert_eq!(text("tanpa data"), "tanpa data");
        assert_eq!(text("a@b tanpa domain"), "a@b tanpa domain");
        assert_eq!(text("😀 0812345678"), "😀 •••678");
    }

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
