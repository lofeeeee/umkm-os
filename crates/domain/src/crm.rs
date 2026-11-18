//! Customer & CRM (P3-3, `PRD.md §4.6`, ADR-0005). Murni: validasi kontak, tag, catatan.

use std::collections::BTreeSet;

use crate::business::{FieldError, is_valid_email, normalize_phone_id, plain_text, push};

/// E.164; nomor lokal Indonesia `08…` / `62…` menjadi `+62…`.
///
/// Nomor dari luar Indonesia (`+65…`, `+1…`) **ditolak**: platform ini khusus UMKM
/// Indonesia, jadi hanya `0…` (lokal) dan `62…` (internasional) yang sah.
pub fn normalize_phone_e164(s: &str) -> Option<String> {
    // `normalize_phone_id` menegakkan "hanya angka" + awalan `0`/`62` + panjang 9–15 digit.
    let n = normalize_phone_id(s)?;
    let e = if let Some(rest) = n.strip_prefix('0') {
        format!("+62{rest}")
    } else {
        format!("+{n}")
    };
    let d = &e[1..];
    (!d.starts_with('0') && (7..=15).contains(&d.len())).then_some(e)
}

pub const NAME_MAX: usize = 120;
pub const NOTE_MAX: usize = 2000;
pub const TAG_MAX: usize = 30;
pub const MAX_TAGS: usize = 20;
pub const MAX_ADDRESSES: usize = 5;
pub const ADDRESS_MAX: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub name: String,
    /// huruf kecil
    pub email: Option<String>,
    /// E.164
    pub phone: Option<String>,
}

fn opt(v: Option<&str>) -> Option<String> {
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Nama wajib; minimal satu dari email/telepon (untuk konfirmasi order).
pub fn validate_contact(
    e: &mut Vec<FieldError>,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
) -> Option<Contact> {
    let n0 = e.len();
    let name = name.trim().to_owned();
    plain_text(e, "name", Some(&name), NAME_MAX, true);
    if name.contains('\n') {
        push(e, "name", "control_character");
    }
    let email = opt(email).map(|s| s.to_lowercase());
    if let Some(m) = &email
        && !is_valid_email(m)
    {
        push(e, "email", "invalid_email");
    }
    let phone = match opt(phone) {
        Some(p) => {
            // Nomor HP hanya angka, diawali `0` (lokal) atau `62` (internasional); disimpan E.164.
            let n = normalize_phone_e164(&p);
            if n.is_none() {
                push(e, "phone", "invalid_phone");
            }
            n
        }
        None => None,
    };
    if email.is_none() && phone.is_none() && e.len() == n0 {
        push(e, "email", "email_or_phone_required");
    }
    (e.len() == n0).then_some(Contact { name, email, phone })
}

/// Tag: huruf kecil, unik, 1..=30 karakter huruf/angka/spasi/`-`/`_`; maks 20.
pub fn normalize_tags(e: &mut Vec<FieldError>, tags: &[String]) -> Vec<String> {
    if tags.len() > MAX_TAGS {
        push(e, "tags", "too_many");
    }
    let mut out = BTreeSet::new();
    for (i, t) in tags.iter().enumerate().take(MAX_TAGS) {
        let t = t.trim().to_lowercase();
        let ok = !t.is_empty()
            && t.chars().count() <= TAG_MAX
            && t.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'));
        if ok {
            out.insert(t);
        } else {
            push(e, format!("tags[{i}]"), "invalid_tag");
        }
    }
    out.into_iter().collect()
}

pub fn validate_note(e: &mut Vec<FieldError>, note: Option<&str>) -> Option<String> {
    let n = opt(note);
    plain_text(e, "internal_note", n.as_deref(), NOTE_MAX, false);
    n
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerAddress {
    pub label: Option<String>,
    pub address: String,
    pub note: Option<String>,
}

pub fn validate_addresses(
    e: &mut Vec<FieldError>,
    list: Vec<CustomerAddress>,
) -> Vec<CustomerAddress> {
    if list.len() > MAX_ADDRESSES {
        push(e, "addresses", "too_many");
    }
    list.into_iter()
        .take(MAX_ADDRESSES)
        .enumerate()
        .map(|(i, a)| {
            let p = format!("addresses[{i}]");
            let address = a.address.trim().to_owned();
            plain_text(
                e,
                &format!("{p}.address"),
                Some(&address),
                ADDRESS_MAX,
                true,
            );
            let label = opt(a.label.as_deref());
            plain_text(e, &format!("{p}.label"), label.as_deref(), 60, false);
            let note = opt(a.note.as_deref());
            plain_text(e, &format!("{p}.note"), note.as_deref(), 200, false);
            CustomerAddress {
                label,
                address,
                note,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_rules() {
        let mut e = vec![];
        let c = validate_contact(
            &mut e,
            " Sari ",
            Some(" Sari@Mail.ID "),
            Some("0812-3456-7890"),
        )
        .unwrap();
        assert_eq!(c.email.as_deref(), Some("sari@mail.id"));
        assert_eq!(c.phone.as_deref(), Some("+6281234567890"));
        let mut e = vec![];
        assert!(validate_contact(&mut e, "Sari", None, Some(" ")).is_none());
        assert_eq!(e[0].code, "email_or_phone_required");
        let mut e = vec![];
        assert!(validate_contact(&mut e, "", Some("x"), Some("12")).is_none());
        let codes: Vec<_> = e.iter().map(|x| x.code).collect();
        assert_eq!(codes, ["required", "invalid_email", "invalid_phone"]);
    }

    #[test]
    fn phone_e164() {
        assert_eq!(
            normalize_phone_e164("0812-3456-7890").as_deref(),
            Some("+6281234567890")
        );
        assert_eq!(
            normalize_phone_e164("6281234567890").as_deref(),
            Some("+6281234567890")
        );
        // Nomor luar Indonesia ditolak (khusus UMKM Indonesia).
        assert!(normalize_phone_e164("+65 9123 4567").is_none());
        // Awalan selain `0`/`62` ditolak.
        assert!(normalize_phone_e164("12345678").is_none());
        assert!(normalize_phone_e164("81234567890").is_none());
        // Huruf/simbol di luar pemisah yang diizinkan ditolak.
        assert!(normalize_phone_e164("0812#4567890").is_none());
    }

    #[test]
    fn tags_normalized_unique() {
        let mut e = vec![];
        let t = normalize_tags(
            &mut e,
            &["VIP".into(), "vip ".into(), "Langganan Pagi".into()],
        );
        assert_eq!(t, ["langganan pagi", "vip"]);
        assert!(e.is_empty());
        let t = normalize_tags(&mut e, &["<b>".into(), "".into()]);
        assert!(t.is_empty());
        assert_eq!(e.len(), 2);
    }
}
