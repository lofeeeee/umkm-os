//! Validator `content_json` halaman website (ADR-0007, `PRD.md §4.3`) — P2-1.
//!
//! Tertutup: setiap objek hanya boleh memuat field yang dikenal; tidak ada field HTML/JS/CSS.
//! Validator total (tidak pernah panic) dan mengumpulkan SEMUA error beserta path-nya.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use uuid::Uuid;

pub const SCHEMA_VERSION: i64 = 1;
pub const MAX_SECTIONS: usize = 30;
/// Batas ukuran dokumen terserialisasi (byte). Body HTTP sendiri dibatasi lebih awal.
pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024;
pub const MAX_URL_LEN: usize = 500;

/// Preset typography (whitelist; bukan nama font bebas).
pub const FONT_PRESETS: [&str; 5] = ["inter", "poppins", "nunito", "merriweather", "system"];

/// 11 komponen awal `PRD.md §4.3`.
pub const SECTION_TYPES: [&str; 11] = [
    "header",
    "hero",
    "about",
    "product_grid",
    "promotion",
    "gallery",
    "testimonials",
    "business_hours",
    "location",
    "contact",
    "footer",
];

pub const SOCIAL_PLATFORMS: [&str; 6] =
    ["instagram", "facebook", "tiktok", "youtube", "x", "website"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    /// JSON pointer-ish, mis. `sections[2].data.title`.
    pub path: String,
    pub code: &'static str,
}

/// Dokumen yang lolos validasi. `canonical` = nilai yang disimpan (key terurut oleh
/// `serde_json::Map`), `asset_refs` = objek storage yang dirujuk (dicek kepemilikannya
/// oleh lapisan application, ADR-0009).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidPage {
    pub canonical: Value,
    pub template: String,
    pub asset_refs: BTreeSet<Uuid>,
}

pub fn validate_page(doc: &Value) -> Result<ValidPage, Vec<FieldError>> {
    let mut v = V::default();
    let size = serde_json::to_vec(doc)
        .map(|b| b.len())
        .unwrap_or(usize::MAX);
    if size > MAX_DOCUMENT_BYTES {
        v.err("", "document_too_large");
        return Err(v.errors);
    }
    let Some(root) = v.object(
        doc,
        "",
        &["schema_version", "template", "theme", "sections"],
    ) else {
        return Err(v.errors);
    };
    match root.get("schema_version").and_then(Value::as_i64) {
        Some(SCHEMA_VERSION) => {}
        _ => v.err("schema_version", "unsupported_schema_version"),
    }
    let template = match root.get("template") {
        Some(Value::String(s)) if is_key(s, 40) => s.clone(),
        _ => {
            v.err("template", "invalid_template_key");
            String::new()
        }
    };
    match root.get("theme") {
        Some(theme) => v.theme(theme),
        None => v.err("theme", "required"),
    }
    match root.get("sections") {
        Some(Value::Array(items)) => {
            if items.len() > MAX_SECTIONS {
                v.err("sections", "too_many_sections");
            }
            let mut ids = BTreeSet::new();
            for (i, s) in items.iter().take(MAX_SECTIONS).enumerate() {
                v.section(s, &format!("sections[{i}]"), &mut ids);
            }
        }
        Some(_) => v.err("sections", "expected_array"),
        None => v.err("sections", "required"),
    }
    if v.errors.is_empty() {
        Ok(ValidPage {
            canonical: doc.clone(),
            template,
            asset_refs: v.assets,
        })
    } else {
        Err(v.errors)
    }
}

/// `[a-z0-9_-]`, tidak kosong, maks `max`.
fn is_key(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// URL yang boleh dipakai di tautan/CTA. Skema berbahaya (`javascript:`, `data:`,
/// `vbscript:`, `file:`) dan protocol-relative `//host` ditolak.
pub fn is_safe_href(s: &str) -> bool {
    if s.is_empty() || s.len() > MAX_URL_LEN || s.chars().any(|c| c.is_control() || c == ' ') {
        return false;
    }
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower
        .strip_prefix("https://")
        .or(lower.strip_prefix("http://"))
    {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        return !host.is_empty() && !host.contains('@') && !host.contains('\\');
    }
    if lower.starts_with("mailto:") {
        return s.len() > 7 && s.contains('@');
    }
    if let Some(num) = lower.strip_prefix("tel:") {
        return !num.is_empty()
            && num
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'(' | b')'));
    }
    if let Some(anchor) = s.strip_prefix('#') {
        return is_key(anchor, 40);
    }
    s.starts_with('/') && !s.starts_with("//") && !s.contains('\\')
}

#[derive(Default)]
struct V {
    errors: Vec<FieldError>,
    assets: BTreeSet<Uuid>,
}

impl V {
    fn err(&mut self, path: &str, code: &'static str) {
        self.errors.push(FieldError {
            path: path.to_owned(),
            code,
        });
    }

    /// Objek dengan field tertutup. Field tak dikenal dilaporkan per path.
    fn object<'a>(
        &mut self,
        value: &'a Value,
        path: &str,
        allowed: &[&str],
    ) -> Option<&'a Map<String, Value>> {
        let Value::Object(map) = value else {
            self.err(path, "expected_object");
            return None;
        };
        for k in map.keys() {
            if !allowed.contains(&k.as_str()) {
                self.err(&join(path, k), "unknown_field");
            }
        }
        Some(map)
    }

    /// Plain text: tanpa karakter kontrol selain baris baru; panjang dalam karakter.
    fn text(
        &mut self,
        map: &Map<String, Value>,
        path: &str,
        key: &str,
        max: usize,
        required: bool,
    ) {
        let p = join(path, key);
        match map.get(key) {
            None if required => self.err(&p, "required"),
            None => {}
            Some(Value::String(s)) => {
                let n = s.chars().count();
                if required && s.trim().is_empty() {
                    self.err(&p, "required");
                } else if n > max {
                    self.err(&p, "too_long");
                } else if s.chars().any(|c| c.is_control() && c != '\n') {
                    self.err(&p, "control_character");
                }
            }
            Some(_) => self.err(&p, "expected_string"),
        }
    }

    fn href(&mut self, map: &Map<String, Value>, path: &str, key: &str, required: bool) {
        let p = join(path, key);
        match map.get(key) {
            None if required => self.err(&p, "required"),
            None => {}
            Some(Value::String(s)) if is_safe_href(s) => {}
            Some(_) => self.err(&p, "unsafe_url"),
        }
    }

    /// Rujukan gambar: hanya `asset:<uuid>` (objek storage tenant, ADR-0009).
    fn asset(&mut self, map: &Map<String, Value>, path: &str, key: &str, required: bool) {
        let p = join(path, key);
        match map.get(key) {
            None if required => self.err(&p, "required"),
            None => {}
            Some(Value::String(s)) => match s.strip_prefix("asset:").map(Uuid::parse_str) {
                Some(Ok(id)) => {
                    self.assets.insert(id);
                }
                _ => self.err(&p, "invalid_asset_ref"),
            },
            Some(_) => self.err(&p, "invalid_asset_ref"),
        }
    }

    fn boolean(&mut self, map: &Map<String, Value>, path: &str, key: &str) {
        if let Some(v) = map.get(key)
            && !v.is_boolean()
        {
            self.err(&join(path, key), "expected_boolean");
        }
    }

    fn int(&mut self, map: &Map<String, Value>, path: &str, key: &str, min: i64, max: i64) {
        let p = join(path, key);
        match map.get(key).map(Value::as_i64) {
            None => self.err(&p, "required"),
            Some(Some(n)) if (min..=max).contains(&n) => {}
            Some(_) => self.err(&p, "out_of_range"),
        }
    }

    /// Array objek; tiap item divalidasi `each`.
    fn list(
        &mut self,
        map: &Map<String, Value>,
        path: &str,
        key: &str,
        (min, max): (usize, usize),
        each: impl Fn(&mut Self, &Value, &str),
    ) {
        let p = join(path, key);
        match map.get(key) {
            None if min == 0 => {}
            None => self.err(&p, "required"),
            Some(Value::Array(items)) => {
                if items.len() < min || items.len() > max {
                    self.err(&p, "invalid_length");
                }
                for (i, item) in items.iter().take(max).enumerate() {
                    each(self, item, &format!("{p}[{i}]"));
                }
            }
            Some(_) => self.err(&p, "expected_array"),
        }
    }

    fn cta(&mut self, map: &Map<String, Value>, path: &str, key: &str) {
        if let Some(c) = map.get(key) {
            let p = join(path, key);
            if let Some(m) = self.object(c, &p, &["label", "href"]) {
                self.text(m, &p, "label", 40, true);
                self.href(m, &p, "href", true);
            }
        }
    }

    fn theme(&mut self, theme: &Value) {
        let Some(m) = self.object(theme, "theme", &["primary", "font"]) else {
            return;
        };
        match m.get("primary") {
            Some(Value::String(s)) if is_hex_color(s) => {}
            _ => self.err("theme.primary", "invalid_color"),
        }
        match m.get("font") {
            Some(Value::String(s)) if FONT_PRESETS.contains(&s.as_str()) => {}
            _ => self.err("theme.font", "invalid_font_preset"),
        }
    }

    fn section(&mut self, s: &Value, path: &str, ids: &mut BTreeSet<String>) {
        let Some(m) = self.object(s, path, &["id", "type", "enabled", "data"]) else {
            return;
        };
        match m.get("id") {
            Some(Value::String(id)) if is_key(id, 40) && !id.contains('_') => {
                if !ids.insert(id.clone()) {
                    self.err(&join(path, "id"), "duplicate_id");
                }
            }
            _ => self.err(&join(path, "id"), "invalid_id"),
        }
        if !m.get("enabled").is_some_and(Value::is_boolean) {
            self.err(&join(path, "enabled"), "expected_boolean");
        }
        let ty = match m.get("type") {
            Some(Value::String(t)) if SECTION_TYPES.contains(&t.as_str()) => t.as_str(),
            _ => {
                self.err(&join(path, "type"), "unknown_section_type");
                return;
            }
        };
        let dp = join(path, "data");
        let Some(data) = m.get("data") else {
            self.err(&dp, "required");
            return;
        };
        self.section_data(ty, data, &dp);
    }

    fn section_data(&mut self, ty: &str, data: &Value, p: &str) {
        let allowed: &[&str] = match ty {
            "header" => &["logo", "title", "nav"],
            "hero" => &["title", "subtitle", "image", "cta"],
            "about" => &["title", "body", "image"],
            "product_grid" => &["title", "limit"],
            "promotion" => &["title", "body", "image", "cta"],
            "gallery" => &["title", "images"],
            "testimonials" => &["title", "items"],
            "business_hours" => &["title"],
            "location" => &["title", "show_map"],
            "contact" => &[
                "title",
                "show_phone",
                "show_email",
                "show_whatsapp",
                "socials",
            ],
            _ => &["text"], // footer
        };
        let Some(m) = self.object(data, p, allowed) else {
            return;
        };
        match ty {
            "header" => {
                self.asset(m, p, "logo", false);
                self.text(m, p, "title", 80, true);
                self.list(m, p, "nav", (0, 8), |v, item, ip| {
                    if let Some(n) = v.object(item, ip, &["label", "href"]) {
                        v.text(n, ip, "label", 40, true);
                        v.href(n, ip, "href", true);
                    }
                });
            }
            "hero" | "promotion" => {
                self.text(m, p, "title", 120, true);
                let body = if ty == "hero" { "subtitle" } else { "body" };
                self.text(m, p, body, if ty == "hero" { 240 } else { 500 }, false);
                self.asset(m, p, "image", false);
                self.cta(m, p, "cta");
            }
            "about" => {
                self.text(m, p, "title", 120, false);
                self.text(m, p, "body", 2000, true);
                self.asset(m, p, "image", false);
            }
            "product_grid" => {
                self.text(m, p, "title", 120, false);
                self.int(m, p, "limit", 1, 24);
            }
            "gallery" => {
                self.text(m, p, "title", 120, false);
                self.list(m, p, "images", (0, 24), |v, item, ip| {
                    if let Some(n) = v.object(item, ip, &["asset", "caption"]) {
                        v.asset(n, ip, "asset", true);
                        v.text(n, ip, "caption", 120, false);
                    }
                });
            }
            "testimonials" => {
                self.text(m, p, "title", 120, false);
                self.list(m, p, "items", (1, 12), |v, item, ip| {
                    if let Some(n) = v.object(item, ip, &["name", "quote"]) {
                        v.text(n, ip, "name", 80, true);
                        v.text(n, ip, "quote", 500, true);
                    }
                });
            }
            "business_hours" => self.text(m, p, "title", 120, false),
            "location" => {
                self.text(m, p, "title", 120, false);
                self.boolean(m, p, "show_map");
            }
            "contact" => {
                self.text(m, p, "title", 120, false);
                for k in ["show_phone", "show_email", "show_whatsapp"] {
                    self.boolean(m, p, k);
                }
                self.list(m, p, "socials", (0, 10), |v, item, ip| {
                    if let Some(n) = v.object(item, ip, &["platform", "url"]) {
                        match n.get("platform") {
                            Some(Value::String(s)) if SOCIAL_PLATFORMS.contains(&s.as_str()) => {}
                            _ => v.err(&join(ip, "platform"), "invalid_platform"),
                        }
                        match n.get("url") {
                            Some(Value::String(s))
                                if s.to_ascii_lowercase().starts_with("https://")
                                    && is_safe_href(s) => {}
                            _ => v.err(&join(ip, "url"), "unsafe_url"),
                        }
                    }
                });
            }
            _ => self.text(m, p, "text", 240, false),
        }
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn valid() -> Value {
        json!({
            "schema_version": 1,
            "template": "modern_store",
            "theme": {"primary": "#2563EB", "font": "inter"},
            "sections": [
                {"id": "header-1", "type": "header", "enabled": true,
                 "data": {"title": "Toko A", "nav": [{"label": "Produk", "href": "#products-1"}]}},
                {"id": "hero-001", "type": "hero", "enabled": true,
                 "data": {"title": "Selamat Datang", "subtitle": "Produk pilihan",
                          "image": "asset:6f9619ff-8b86-d011-b42d-00c04fc964ff",
                          "cta": {"label": "Belanja", "href": "/produk"}}},
                {"id": "about-1", "type": "about", "enabled": false, "data": {"body": "Sejak 2020"}},
                {"id": "products-1", "type": "product_grid", "enabled": true, "data": {"limit": 8}},
                {"id": "promo-1", "type": "promotion", "enabled": true, "data": {"title": "Diskon"}},
                {"id": "gallery-1", "type": "gallery", "enabled": true,
                 "data": {"images": [{"asset": "asset:6f9619ff-8b86-d011-b42d-00c04fc964fe", "caption": "Toko"}]}},
                {"id": "testi-1", "type": "testimonials", "enabled": true,
                 "data": {"items": [{"name": "Budi", "quote": "Mantap <b>sekali</b>"}]}},
                {"id": "hours-1", "type": "business_hours", "enabled": true, "data": {}},
                {"id": "loc-1", "type": "location", "enabled": true, "data": {"show_map": true}},
                {"id": "contact-1", "type": "contact", "enabled": true,
                 "data": {"show_whatsapp": true, "socials": [{"platform": "instagram", "url": "https://instagram.com/toko"}]}},
                {"id": "footer-1", "type": "footer", "enabled": true, "data": {"text": "© Toko A"}}
            ]
        })
    }

    fn codes(doc: &Value) -> Vec<(String, &'static str)> {
        validate_page(doc)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.path, e.code))
            .collect()
    }

    #[test]
    fn accepts_all_eleven_section_types_and_collects_assets() {
        let ok = validate_page(&valid()).expect("valid");
        assert_eq!(ok.template, "modern_store");
        assert_eq!(ok.asset_refs.len(), 2);
        let types: BTreeSet<_> = valid()["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["type"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(types.len(), SECTION_TYPES.len());
    }

    #[test]
    fn html_text_is_allowed_as_plain_text() {
        // AC-WEB-018: teks berisi tag diterima sebagai teks; renderer menampilkannya literal.
        let mut d = valid();
        d["sections"][1]["data"]["title"] = json!("<script>alert(1)</script>");
        assert!(validate_page(&d).is_ok());
    }

    #[test]
    fn rejects_unknown_fields_everywhere() {
        let mut d = valid();
        d["html"] = json!("<b>x</b>");
        d["theme"]["css"] = json!("body{}");
        d["sections"][1]["data"]["onclick"] = json!("alert(1)");
        d["sections"][1]["style"] = json!("x");
        let c = codes(&d);
        for p in [
            "html",
            "theme.css",
            "sections[1].data.onclick",
            "sections[1].style",
        ] {
            assert!(c.contains(&(p.to_owned(), "unknown_field")), "{p}: {c:?}");
        }
    }

    #[test]
    fn rejects_unknown_type_version_and_dup_ids() {
        let mut d = valid();
        d["schema_version"] = json!(2);
        d["sections"][0]["type"] = json!("raw_html");
        d["sections"][2]["id"] = json!("hero-001");
        let c = codes(&d);
        assert!(c.contains(&("schema_version".into(), "unsupported_schema_version")));
        assert!(c.contains(&("sections[0].type".into(), "unknown_section_type")));
        assert!(c.contains(&("sections[2].id".into(), "duplicate_id")));
    }

    #[test]
    fn rejects_dangerous_urls() {
        for bad in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>",
            "vbscript:x",
            "//evil.example",
            "https://user@evil",
            "java\tscript:x",
            " https://x.id",
            "file:///etc/passwd",
            "https://",
        ] {
            let mut d = valid();
            d["sections"][1]["data"]["cta"]["href"] = json!(bad);
            assert_eq!(
                codes(&d),
                vec![("sections[1].data.cta.href".into(), "unsafe_url")],
                "{bad}"
            );
        }
        for good in [
            "https://x.id/a?b=1",
            "http://x.id",
            "/produk",
            "#hero-001",
            "mailto:a@b.id",
            "tel:+62-812",
        ] {
            assert!(is_safe_href(good), "{good}");
        }
    }

    #[test]
    fn rejects_limits_theme_and_assets() {
        let mut d = valid();
        d["theme"] = json!({"primary": "blue", "font": "Comic Sans"});
        d["sections"][1]["data"]["title"] = json!("x".repeat(121));
        d["sections"][3]["data"]["limit"] = json!(100);
        d["sections"][1]["data"]["image"] = json!("https://evil/x.png");
        d["sections"][4]["data"]["title"] = json!("a\u{0007}b");
        let c = codes(&d);
        for want in [
            ("theme.primary", "invalid_color"),
            ("theme.font", "invalid_font_preset"),
            ("sections[1].data.title", "too_long"),
            ("sections[3].data.limit", "out_of_range"),
            ("sections[1].data.image", "invalid_asset_ref"),
            ("sections[4].data.title", "control_character"),
        ] {
            assert!(c.contains(&(want.0.to_owned(), want.1)), "{want:?}: {c:?}");
        }
    }

    #[test]
    fn rejects_too_many_sections_and_large_documents() {
        let mut d = valid();
        let s = d["sections"][3].clone();
        let many: Vec<Value> = (0..31)
            .map(|i| {
                let mut x = s.clone();
                x["id"] = json!(format!("p-{i}"));
                x
            })
            .collect();
        d["sections"] = json!(many);
        assert_eq!(codes(&d), vec![("sections".into(), "too_many_sections")]);
        let mut big = valid();
        big["sections"][2]["data"]["body"] = json!("x".repeat(MAX_DOCUMENT_BYTES));
        assert_eq!(codes(&big), vec![("".into(), "document_too_large")]);
    }

    #[test]
    fn non_object_roots_are_rejected() {
        for d in [json!(null), json!([]), json!("x"), json!(1)] {
            assert_eq!(codes(&d), vec![("".into(), "expected_object")]);
        }
    }
}
