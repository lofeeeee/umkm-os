//! AC-WEB-006: validator total terhadap input acak (P2-1).
//! Generator deterministik (xorshift, seed tetap) agar kegagalan dapat direproduksi
//! tanpa dependency fuzzing tambahan.

use serde_json::{Map, Value, json};
use umkm_domain::website::{schema::is_safe_href, validate_page};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

const KEYS: &[&str] = &[
    "schema_version",
    "template",
    "theme",
    "sections",
    "primary",
    "font",
    "id",
    "type",
    "enabled",
    "data",
    "title",
    "subtitle",
    "body",
    "image",
    "cta",
    "label",
    "href",
    "nav",
    "logo",
    "limit",
    "images",
    "asset",
    "caption",
    "items",
    "name",
    "quote",
    "show_map",
    "socials",
    "platform",
    "url",
    "text",
    "html",
    "script",
    "__proto__",
    "",
];
const STRINGS: &[&str] = &[
    "",
    "hero",
    "header",
    "gallery",
    "footer",
    "modern_store",
    "#2563EB",
    "inter",
    "javascript:alert(1)",
    "https://x.id",
    "/a",
    "//evil",
    "asset:6f9619ff-8b86-d011-b42d-00c04fc964ff",
    "asset:zzz",
    "<script>alert(1)</script>",
    "\u{0000}",
    "é😀",
    "data:text/html,x",
    "tel:+62",
    "mailto:a@b.id",
    "#x",
    "hero-001",
];

fn random_value(r: &mut Rng, depth: u32) -> Value {
    match r.below(if depth > 4 { 5 } else { 8 }) {
        0 => Value::Null,
        1 => json!(r.below(2) == 0),
        2 => json!(r.next() as i64 % 50 - 10),
        3 => json!(f64::from(r.below(1000) as u32) / 7.0),
        4 => {
            let s = *r.pick(STRINGS);
            if r.below(10) == 0 {
                json!(s.repeat(r.below(600) as usize))
            } else {
                json!(s)
            }
        }
        5 => Value::Array(
            (0..r.below(6))
                .map(|_| random_value(r, depth + 1))
                .collect(),
        ),
        _ => {
            let mut m = Map::new();
            for _ in 0..r.below(7) {
                m.insert((*r.pick(KEYS)).to_owned(), random_value(r, depth + 1));
            }
            Value::Object(m)
        }
    }
}

fn base() -> Value {
    json!({
        "schema_version": 1, "template": "modern_store",
        "theme": {"primary": "#2563EB", "font": "inter"},
        "sections": [
            {"id": "hero-001", "type": "hero", "enabled": true,
             "data": {"title": "Hi", "cta": {"label": "Go", "href": "/x"}}},
            {"id": "contact-1", "type": "contact", "enabled": true,
             "data": {"socials": [{"platform": "x", "url": "https://x.com/a"}]}}
        ]
    })
}

/// Ganti satu node acak di dalam dokumen dengan nilai acak.
fn mutate(r: &mut Rng, v: &mut Value, depth: u32) {
    let replace = depth > 0 && r.below(4) == 0;
    match v {
        _ if replace => *v = random_value(r, depth),
        Value::Object(m) if !m.is_empty() => {
            if r.below(5) == 0 {
                m.insert((*r.pick(KEYS)).to_owned(), random_value(r, depth + 1));
            } else {
                let k = m
                    .keys()
                    .nth(r.below(m.len() as u64) as usize)
                    .cloned()
                    .unwrap();
                mutate(r, m.get_mut(&k).unwrap(), depth + 1);
            }
        }
        Value::Array(a) if !a.is_empty() => {
            let i = r.below(a.len() as u64) as usize;
            mutate(r, &mut a[i], depth + 1);
        }
        _ => *v = random_value(r, depth),
    }
}

/// Setiap string di dokumen yang DITERIMA dan berada di field tautan wajib aman.
fn assert_accepted_is_safe(v: &Value, key: &str) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                assert!(
                    !["html", "script", "__proto__", ""].contains(&k.as_str()),
                    "field {k} lolos"
                );
                assert_accepted_is_safe(x, k);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| assert_accepted_is_safe(x, key)),
        Value::String(s) if key == "href" || key == "url" => assert!(is_safe_href(s), "{s}"),
        Value::String(s) => assert!(!s.contains('\u{0000}'), "kontrol lolos di {key}"),
        _ => {}
    }
}

#[test]
fn ten_thousand_random_documents_never_panic() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let mut accepted = 0;
    for _ in 0..10_000 {
        let doc = random_value(&mut r, 0);
        if let Ok(ok) = validate_page(&doc) {
            accepted += 1;
            assert_accepted_is_safe(&ok.canonical, "");
        }
    }
    // Dokumen acak murni hampir mustahil valid; yang penting: total, tanpa panic.
    assert!(accepted < 10, "accepted={accepted}");
}

#[test]
fn ten_thousand_mutations_of_a_valid_document() {
    let mut r = Rng(0xD1B5_4A32_D192_ED03);
    assert!(validate_page(&base()).is_ok());
    let (mut ok, mut rejected) = (0, 0);
    for _ in 0..10_000 {
        let mut doc = base();
        for _ in 0..=r.below(3) {
            mutate(&mut r, &mut doc, 0);
        }
        match validate_page(&doc) {
            Ok(v) => {
                ok += 1;
                assert_accepted_is_safe(&v.canonical, "");
                // Idempoten: dokumen kanonik tetap valid.
                assert!(validate_page(&v.canonical).is_ok());
            }
            Err(errs) => {
                rejected += 1;
                assert!(!errs.is_empty());
            }
        }
    }
    assert!(ok > 0 && rejected > 0, "ok={ok} rejected={rejected}");
}
