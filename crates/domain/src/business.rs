//! Business profile, alamat, jam operasional (P2-5, `PRD.md §4.4`). Murni.
//!
//! Waktu lokal tenant dihitung PostgreSQL dari timezone IANA (`now() AT TIME ZONE tz`);
//! domain hanya menerima (tanggal lokal, hari, menit) sehingga aturan buka/tutup dapat
//! diuji tanpa jam sistem.

use crate::website::schema::{SOCIAL_PLATFORMS, is_safe_href};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub path: String,
    pub code: &'static str,
}

fn err(errors: &mut Vec<FieldError>, path: impl Into<String>, code: &'static str) {
    errors.push(FieldError {
        path: path.into(),
        code,
    });
}

/// Menit sejak 00:00, 0..1440.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Minute(u16);

impl Minute {
    /// `HH:MM` 24 jam.
    pub fn parse(s: &str) -> Option<Self> {
        let (h, m) = s.split_once(':')?;
        if h.len() != 2 || m.len() != 2 {
            return None;
        }
        let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
        (h < 24 && m < 60).then_some(Self(h * 60 + m))
    }
    pub fn from_minutes(m: u16) -> Option<Self> {
        (m < 1440).then_some(Self(m))
    }
    pub fn get(self) -> u16 {
        self.0
    }
    pub fn format(self) -> String {
        format!("{:02}:{:02}", self.0 / 60, self.0 % 60)
    }
}

/// Rentang jam; `close < open` = tutup lewat tengah malam (hari berikutnya).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub open: Minute,
    pub close: Minute,
}

impl Span {
    pub fn new(open: Minute, close: Minute) -> Option<Self> {
        (open != close).then_some(Self { open, close })
    }
    pub fn overnight(self) -> bool {
        self.close < self.open
    }
    /// Apakah `t` (hari yang sama dengan pembukaan) termasuk bagian hari ini.
    fn covers_same_day(self, t: Minute) -> bool {
        if self.overnight() {
            t >= self.open
        } else {
            self.open <= t && t < self.close
        }
    }
    /// Apakah `t` di hari BERIKUTNYA masih dalam sisa rentang overnight.
    fn covers_spillover(self, t: Minute) -> bool {
        self.overnight() && t < self.close
    }
}

/// ISO weekday: 1 = Senin … 7 = Minggu.
pub type Weekday = u8;

#[derive(Debug, Clone, Default)]
pub struct WeeklyHours {
    days: [Option<Span>; 7],
}

impl WeeklyHours {
    pub fn set(&mut self, day: Weekday, span: Span) {
        if (1..=7).contains(&day) {
            self.days[usize::from(day - 1)] = Some(span);
        }
    }
    pub fn get(&self, day: Weekday) -> Option<Span> {
        (1..=7)
            .contains(&day)
            .then(|| self.days[usize::from(day - 1)])
            .flatten()
    }
}

/// Exception untuk satu tanggal lokal: tutup, atau jam khusus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayOverride {
    Closed,
    Hours(Span),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenStatus {
    Open,
    Closed,
}

/// Status buka/tutup pada waktu lokal tenant (AC-BIZ-005/006).
///
/// `today`/`yesterday` = override exception untuk tanggal lokal hari ini / kemarin
/// (kemarin relevan untuk sisa jam overnight).
pub fn status_at(
    weekly: &WeeklyHours,
    weekday: Weekday,
    now: Minute,
    today: Option<DayOverride>,
    yesterday: Option<DayOverride>,
) -> OpenStatus {
    let prev_day = if weekday == 1 { 7 } else { weekday - 1 };
    let today_span = match today {
        Some(DayOverride::Closed) => None,
        Some(DayOverride::Hours(s)) => Some(s),
        None => weekly.get(weekday),
    };
    let prev_span = match yesterday {
        Some(DayOverride::Closed) => None,
        Some(DayOverride::Hours(s)) => Some(s),
        None => weekly.get(prev_day),
    };
    let open = today_span.is_some_and(|s| s.covers_same_day(now))
        || prev_span.is_some_and(|s| s.covers_spillover(now));
    if open {
        OpenStatus::Open
    } else {
        OpenStatus::Closed
    }
}

// ------------------------------------------------------------------ validasi profil

pub const NAME_MAX: usize = 120;
pub const DESCRIPTION_MAX: usize = 2000;
pub const ADDRESS_MAX: usize = 500;
pub const MAX_ADDRESSES: usize = 10;
pub const MAX_SOCIALS: usize = 10;

/// Batas jumlah digit nomor telepon (tanpa `+`): 9–15 (E.164).
pub const PHONE_MIN_DIGITS: usize = 9;
pub const PHONE_MAX_DIGITS: usize = 15;

/// Email sederhana: satu `@`, lokal & domain tak kosong, domain punya titik, tanpa spasi.
pub fn is_valid_email(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.len() > 254 || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    // Bagian lokal: 1–64 karakter, tanpa titik di tepi/berurutan.
    if local.is_empty() || local.len() > 64 || local.starts_with('.') || local.ends_with('.') {
        return false;
    }
    if local.contains("..")
        || !local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+/=?^_`{|}~.-".contains(c))
    {
        return false;
    }
    // Domain: label dipisah titik, tiap label tanpa tanda hubung di tepi; TLD ≥ 2 huruf.
    if domain.is_empty() || domain.len() > 253 || domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    for l in &labels {
        if l.is_empty() || l.len() > 63 || l.starts_with('-') || l.ends_with('-') {
            return false;
        }
        if !l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
    }
    let tld = labels[labels.len() - 1];
    tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
}

/// Nomor telepon Indonesia: **hanya angka**, diawali `0` (lokal) atau `62` (internasional).
///
/// Format(spasi, `-`, `.`, `(`, `)`) diabaikan; huruf/simbol lain ditolak. `+` hanya boleh
/// tepat sebelum `62` (mis. `+62 812-3456-7890`). Hasil = digit kanonik tanpa `+`.
pub fn normalize_phone_id(s: &str) -> Option<String> {
    let t: String = s
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '.' | '(' | ')'))
        .collect();
    let digits = match t.strip_prefix('+') {
        Some(rest) if rest.starts_with("62") => rest,
        Some(_) => return None,
        None => t.as_str(),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let prefix_ok = digits.starts_with('0') || digits.starts_with("62");
    let len_ok = (PHONE_MIN_DIGITS..=PHONE_MAX_DIGITS).contains(&digits.len());
    (prefix_ok && len_ok).then(|| digits.to_owned())
}

/// Sama seperti `normalize_phone_id`, tapi langsung tolak bila tidak sah.
pub fn is_valid_phone(s: &str) -> bool {
    normalize_phone_id(s).is_some()
}

/// Nomor telepon/WhatsApp: opsional `+`, lalu 8–15 digit (E.164); spasi/`-` diabaikan.
pub fn normalize_phone(s: &str) -> Option<String> {
    let t: String = s.chars().filter(|c| !matches!(c, ' ' | '-')).collect();
    let digits = t.strip_prefix('+').unwrap_or(&t);
    (digits.len() >= 8 && digits.len() <= 15 && digits.bytes().all(|b| b.is_ascii_digit())).then(
        || {
            if t.starts_with('+') {
                t.clone()
            } else {
                digits.to_owned()
            }
        },
    )
}

pub fn plain_text(
    errors: &mut Vec<FieldError>,
    path: &str,
    v: Option<&str>,
    max: usize,
    required: bool,
) {
    match v {
        None if required => err(errors, path, "required"),
        None => {}
        Some(s) if required && s.trim().is_empty() => err(errors, path, "required"),
        Some(s) if s.chars().count() > max => err(errors, path, "too_long"),
        Some(s) if s.chars().any(|c| c.is_control() && c != '\n') => {
            err(errors, path, "control_character")
        }
        Some(_) => {}
    }
}

pub fn validate_social(errors: &mut Vec<FieldError>, path: &str, platform: &str, url: &str) {
    if !SOCIAL_PLATFORMS.contains(&platform) {
        err(errors, format!("{path}.platform"), "invalid_platform");
    }
    if !(url.to_ascii_lowercase().starts_with("https://") && is_safe_href(url)) {
        err(errors, format!("{path}.url"), "unsafe_url");
    }
}

pub fn validate_coordinates(
    errors: &mut Vec<FieldError>,
    path: &str,
    lat: Option<f64>,
    lng: Option<f64>,
) {
    match (lat, lng) {
        (None, None) => {}
        (Some(a), Some(b)) => {
            if !(a.is_finite() && (-90.0..=90.0).contains(&a)) {
                err(errors, format!("{path}.latitude"), "out_of_range");
            }
            if !(b.is_finite() && (-180.0..=180.0).contains(&b)) {
                err(errors, format!("{path}.longitude"), "out_of_range");
            }
        }
        _ => err(errors, path, "latitude_longitude_pair"),
    }
}

/// Error-builder publik agar lapisan atas memakai format yang sama.
pub fn push(errors: &mut Vec<FieldError>, path: impl Into<String>, code: &'static str) {
    err(errors, path, code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Minute {
        Minute::parse(s).unwrap()
    }
    fn span(a: &str, b: &str) -> Span {
        Span::new(m(a), m(b)).unwrap()
    }

    #[test]
    fn minute_parsing() {
        assert_eq!(m("00:00").get(), 0);
        assert_eq!(m("23:59").get(), 1439);
        for bad in ["24:00", "7:00", "07:60", "0700", "aa:bb", "", "07:00:00"] {
            assert!(Minute::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(m("08:05").format(), "08:05");
        assert!(
            Span::new(m("08:00"), m("08:00")).is_none(),
            "open == close ditolak"
        );
    }

    #[test]
    fn regular_hours_open_and_close_boundaries() {
        // AC-BIZ-006: 08:00–17:00 WIB; 16:30 buka, 17:00 tutup (eksklusif), 07:59 tutup.
        let mut w = WeeklyHours::default();
        w.set(3, span("08:00", "17:00"));
        assert_eq!(status_at(&w, 3, m("16:30"), None, None), OpenStatus::Open);
        assert_eq!(status_at(&w, 3, m("08:00"), None, None), OpenStatus::Open);
        assert_eq!(status_at(&w, 3, m("17:00"), None, None), OpenStatus::Closed);
        assert_eq!(status_at(&w, 3, m("07:59"), None, None), OpenStatus::Closed);
        assert_eq!(
            status_at(&w, 4, m("10:00"), None, None),
            OpenStatus::Closed,
            "hari tanpa jam"
        );
    }

    #[test]
    fn overnight_spills_into_next_day_including_sunday_to_monday() {
        let mut w = WeeklyHours::default();
        w.set(7, span("18:00", "02:00")); // Minggu malam s/d Senin 02:00
        assert_eq!(status_at(&w, 7, m("23:00"), None, None), OpenStatus::Open);
        assert_eq!(status_at(&w, 1, m("01:59"), None, None), OpenStatus::Open);
        assert_eq!(status_at(&w, 1, m("02:00"), None, None), OpenStatus::Closed);
        assert_eq!(status_at(&w, 7, m("17:59"), None, None), OpenStatus::Closed);
    }

    #[test]
    fn exceptions_override_weekly_hours() {
        // AC-BIZ-005
        let mut w = WeeklyHours::default();
        w.set(5, span("08:00", "17:00"));
        assert_eq!(
            status_at(&w, 5, m("10:00"), Some(DayOverride::Closed), None),
            OpenStatus::Closed
        );
        let special = DayOverride::Hours(span("12:00", "14:00"));
        assert_eq!(
            status_at(&w, 5, m("10:00"), Some(special), None),
            OpenStatus::Closed
        );
        assert_eq!(
            status_at(&w, 5, m("13:00"), Some(special), None),
            OpenStatus::Open
        );
        // Libur kemarin membatalkan sisa jam overnight kemarin
        let mut n = WeeklyHours::default();
        n.set(5, span("20:00", "03:00"));
        assert_eq!(status_at(&n, 6, m("01:00"), None, None), OpenStatus::Open);
        assert_eq!(
            status_at(&n, 6, m("01:00"), None, Some(DayOverride::Closed)),
            OpenStatus::Closed
        );
    }

    #[test]
    fn contact_validation() {
        assert!(is_valid_email("a@toko.id"));
        for bad in ["a", "a@", "@b.id", "a@b", "a b@c.id", "a@b.id.", "a@@b.id"] {
            assert!(!is_valid_email(bad), "{bad}");
        }
        assert_eq!(
            normalize_phone("+62 812-3456-7890").as_deref(),
            Some("+6281234567890")
        );
        assert_eq!(
            normalize_phone("081234567890").as_deref(),
            Some("081234567890")
        );
        for bad in ["123", "+62abc45678", "0812345678901234567", ""] {
            assert!(normalize_phone(bad).is_none(), "{bad}");
        }
        let mut e = vec![];
        validate_social(&mut e, "s[0]", "instagram", "javascript:alert(1)");
        validate_social(&mut e, "s[1]", "myspace", "https://x.id");
        validate_social(&mut e, "s[2]", "facebook", "http://fb.com/x");
        assert_eq!(e.len(), 3, "{e:?}");
    }

    #[test]
    fn strict_email_rules() {
        // Sah.
        for ok in [
            "a@toko.id",
            "budi.santoso@umkm.co.id",
            "budi+tag@umkm.co.id",
            "budi_santoso@sub.domain.umkm.co.id",
            "BUDI@TOKO.ID",
            "budi@x-y.id",
        ] {
            assert!(is_valid_email(ok), "{ok}");
        }
        // Tidak sah: satu `@`, domain tanpa titik, TLD non-huruf, spasi, titik ganda/tepi.
        for bad in [
            "a", "a@", "@b.id", "a@b", "a b@c.id", "a@b.id.", "a@@b.id", "a@b..id", ".a@b.id",
            "a.@b.id", "a@b.id.c", "a@b.c", "a@b.1id", "a@-b.id", "a@b-.id", "a(b@b.id",
        ] {
            assert!(!is_valid_email(bad), "{bad}");
        }
        // Whitespace di tepi di-trim (konsisten dengan `opt_trim` di API), bukan ditolak.
        assert!(is_valid_email("a@b.id\n"));
        assert!(is_valid_email("  a@b.id  "));
        // Panjang maksimum: total ≤ 254, lokal ≤ 64.
        assert!(!is_valid_email(&format!("{}@x.id", "a".repeat(65))));
        assert!(is_valid_email(&format!("{}@x.id", "a".repeat(64))));
    }

    #[test]
    fn strict_phone_rules() {
        // Sah: lokal `0…` dan internasional `62…`; pemisah hanya spasi/`-`/`.`/kurung.
        for (input, expect) in [
            ("081234567890", "081234567890"),
            ("0812-3456-7890", "081234567890"),
            ("0812 3456 7890", "081234567890"),
            ("(0812) 3456-7890", "081234567890"),
            ("+6281234567890", "6281234567890"),
            ("+62 812-3456-7890", "6281234567890"),
            ("6281234567890", "6281234567890"),
            ("62 812 3456 7890", "6281234567890"),
        ] {
            assert_eq!(
                normalize_phone_id(input).as_deref(),
                Some(expect),
                "{input}"
            );
        }
        // Tidak sah: awalan lain, huruf, simbol, panjang di luar 9–15 digit.
        for bad in [
            "81234567890",
            "123456789",
            "0812345678901234567",
            "08123abc890",
            "0812#4567890",
            "",
            "0",
        ] {
            assert!(!is_valid_phone(bad), "{bad}");
        }
        // `+` hanya sah sebelum 62.
        assert!(!is_valid_phone("+081234567890"));
        assert!(!is_valid_phone("+1 812 345 6789"));
    }

    #[test]
    fn coordinates() {
        let mut e = vec![];
        validate_coordinates(&mut e, "a", Some(-6.2), Some(106.8));
        validate_coordinates(&mut e, "b", None, None);
        assert!(e.is_empty());
        validate_coordinates(&mut e, "c", Some(91.0), Some(0.0));
        validate_coordinates(&mut e, "d", Some(0.0), Some(-181.0));
        validate_coordinates(&mut e, "e", Some(0.0), None);
        validate_coordinates(&mut e, "f", Some(f64::NAN), Some(0.0));
        let codes: Vec<_> = e.iter().map(|x| (x.path.as_str(), x.code)).collect();
        assert_eq!(
            codes,
            vec![
                ("c.latitude", "out_of_range"),
                ("d.longitude", "out_of_range"),
                ("e", "latitude_longitude_pair"),
                ("f.latitude", "out_of_range")
            ]
        );
    }
}
