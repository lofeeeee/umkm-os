//! Validator `content_json` Studio (`schema_version = 2`, ADR-0017, ADR-0019) — P9-2.
//!
//! Sama ketatnya dengan v1: objek tertutup, teks polos, URL whitelist, aset hanya `asset:<uuid>`.
//! Tambahan v2: kanvas grid 12 kolom (`blocks[]` dengan x/y/w/h), plugin resmi (manifest ADR-0019),
//! larangan tumpang tindih untuk blok grid, batas jumlah per jenis, dan blok SEO.
//! Total (tidak pernah panic) dan mengumpulkan SEMUA error beserta path-nya.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::schema::{FieldError, MAX_DOCUMENT_BYTES, V, ValidPage, is_key, join};

pub const SCHEMA_VERSION: i64 = 2;
pub const COLS: i64 = 12;
pub const MAX_BLOCKS: usize = 40;
pub const MAX_Y: i64 = 2000;
pub const MAX_H: i64 = 400;

/// Plugin resmi (ADR-0019). Data 9 jenis pertama memakai aturan section v1.
pub const PLUGIN_KINDS: [&str; 12] = [
    "hero",
    "about",
    "product_grid",
    "promotion",
    "gallery",
    "testimonials",
    "business_hours",
    "location",
    "contact",
    "text",
    "chatbot",
    "whatsapp",
];
pub const STYLES: [&str; 4] = ["plain", "card", "tint", "dark"];
pub const SURFACES: [&str; 3] = ["light", "cream", "dark"];
pub const TEXT_SCALES: [f64; 3] = [0.9, 1.0, 1.1];
pub const HIDE_ON: [&str; 2] = ["mobile", "desktop"];

/// Varian tata letak per plugin; plugin tanpa varian tidak boleh mengirim `layout`.
pub fn layouts(kind: &str) -> &'static [&'static str] {
    match kind {
        "product_grid" => &["grid", "carousel", "list"],
        "chatbot" => &["inline", "floating", "bar"],
        "whatsapp" => &["floating", "inline"],
        _ => &[],
    }
}

/// Batas jumlah per halaman (ADR-0019).
pub fn max_per_page(kind: &str) -> usize {
    match kind {
        "chatbot" | "whatsapp" => 1,
        _ => MAX_BLOCKS,
    }
}

/// Paket minimum plugin (ADR-0019). Fase 1 Studio: semua plugin `starter`; kandidat `pro` ditambah di sini dan di
/// `frontend/src/studio/plugin-manifest.json` (dijaga tes kontrak).
pub fn plugin_plan(kind: &str) -> &'static str {
    if PLUGIN_KINDS.contains(&kind) {
        "starter"
    } else {
        "pro" // tak dikenal: perlakukan paling ketat (fail-closed)
    }
}

/// ADR-0019: blok yang butuh paket lebih tinggi TIDAK dihapus dari draf/versi terbit (naik paket → tampil lagi),
/// hanya dibuang dari dokumen yang dikirim ke situs publik. Mengembalikan jumlah blok yang disembunyikan.
pub fn hide_unentitled_blocks(
    content: &mut Value,
    has_pro: bool,
    plan_of: impl Fn(&str) -> &'static str,
) -> usize {
    if has_pro || content.get("schema_version").and_then(Value::as_i64) != Some(SCHEMA_VERSION) {
        return 0;
    }
    let Some(Value::Array(blocks)) = content.get_mut("blocks") else {
        return 0;
    };
    let before = blocks.len();
    blocks.retain(|b| {
        plan_of(b.get("kind").and_then(Value::as_str).unwrap_or_default()) == "starter"
    });
    before - blocks.len()
}

/// Blok "embed" berada di luar grid (gelembung/bilah) sehingga dikecualikan dari cek tumpang tindih.
pub fn is_embed(kind: &str, layout: Option<&str>) -> bool {
    matches!(
        (kind, layout),
        ("chatbot", Some("floating" | "bar")) | ("whatsapp", Some("floating"))
    )
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

impl Rect {
    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

pub fn validate(doc: &Value) -> Result<ValidPage, Vec<FieldError>> {
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
        &["schema_version", "template", "theme", "seo", "blocks"],
    ) else {
        return Err(v.errors);
    };
    if root.get("schema_version").and_then(Value::as_i64) != Some(SCHEMA_VERSION) {
        v.err("schema_version", "unsupported_schema_version");
    }
    let template = match root.get("template") {
        Some(Value::String(s)) if is_key(s, 40) => s.clone(),
        _ => {
            v.err("template", "invalid_template_key");
            String::new()
        }
    };
    match root.get("theme") {
        Some(t) => theme(&mut v, t),
        None => v.err("theme", "required"),
    }
    if let Some(seo) = root.get("seo")
        && let Some(m) = v.object(seo, "seo", &["title", "description", "image", "favicon"])
    {
        v.text(m, "seo", "title", 70, false);
        v.text(m, "seo", "description", 160, false);
        v.asset(m, "seo", "image", false);
        v.asset(m, "seo", "favicon", false);
    }
    match root.get("blocks") {
        Some(Value::Array(items)) => blocks(&mut v, items),
        Some(_) => v.err("blocks", "expected_array"),
        None => v.err("blocks", "required"),
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

fn theme(v: &mut V, t: &Value) {
    let Some(m) = v.object(t, "theme", &["primary", "font", "surface", "text_scale"]) else {
        return;
    };
    // primary + font: aturan v1 (dipanggil lewat objek tanpa field v2 agar tidak dilaporkan unknown).
    let mut base = Map::new();
    for k in ["primary", "font"] {
        if let Some(x) = m.get(k) {
            base.insert(k.into(), x.clone());
        }
    }
    v.theme(&Value::Object(base));
    match m.get("surface") {
        None => {}
        Some(Value::String(s)) if SURFACES.contains(&s.as_str()) => {}
        Some(_) => v.err("theme.surface", "invalid_surface"),
    }
    match m.get("text_scale") {
        None => {}
        Some(Value::Number(n))
            if n.as_f64()
                .is_some_and(|f| TEXT_SCALES.iter().any(|t| (t - f).abs() < 1e-9)) => {}
        Some(_) => v.err("theme.text_scale", "invalid_text_scale"),
    }
}

fn int_in(
    v: &mut V,
    m: &Map<String, Value>,
    p: &str,
    key: &str,
    min: i64,
    max: i64,
) -> Option<i64> {
    match m.get(key).map(Value::as_i64) {
        Some(Some(n)) if (min..=max).contains(&n) => Some(n),
        None => {
            v.err(&join(p, key), "required");
            None
        }
        Some(_) => {
            v.err(&join(p, key), "out_of_range");
            None
        }
    }
}

fn blocks(v: &mut V, items: &[Value]) {
    if items.len() > MAX_BLOCKS {
        v.err("blocks", "too_many_blocks");
    }
    let mut ids = BTreeSet::new();
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut grid: Vec<(usize, Rect)> = Vec::new();
    for (i, b) in items.iter().take(MAX_BLOCKS).enumerate() {
        let p = format!("blocks[{i}]");
        let Some(m) = v.object(
            b,
            &p,
            &[
                "id",
                "kind",
                "x",
                "y",
                "w",
                "h",
                "locked",
                "auto_height",
                "style",
                "layout",
                "chat_open",
                "hide_on",
                "data",
            ],
        ) else {
            continue;
        };
        match m.get("id") {
            Some(Value::String(id)) if is_key(id, 40) && !id.contains('_') => {
                if !ids.insert(id.clone()) {
                    v.err(&join(&p, "id"), "duplicate_id");
                }
            }
            _ => v.err(&join(&p, "id"), "invalid_id"),
        }
        let kind: &'static str = match m.get("kind") {
            Some(Value::String(k)) => match PLUGIN_KINDS.iter().find(|x| **x == k.as_str()) {
                Some(k) => k,
                None => {
                    v.err(&join(&p, "kind"), "unknown_plugin_kind");
                    continue;
                }
            },
            _ => {
                v.err(&join(&p, "kind"), "unknown_plugin_kind");
                continue;
            }
        };
        let n = counts.entry(kind).or_default();
        *n += 1;
        if *n > max_per_page(kind) {
            v.err(&join(&p, "kind"), "too_many_of_kind");
        }
        for k in ["locked", "auto_height", "chat_open"] {
            v.boolean(m, &p, k);
        }
        if m.contains_key("chat_open") && kind != "chatbot" {
            v.err(&join(&p, "chat_open"), "unknown_field");
        }
        match m.get("style") {
            Some(Value::String(s)) if STYLES.contains(&s.as_str()) => {}
            _ => v.err(&join(&p, "style"), "invalid_style"),
        }
        match m.get("hide_on") {
            None => {}
            Some(Value::String(s)) if HIDE_ON.contains(&s.as_str()) => {}
            Some(_) => v.err(&join(&p, "hide_on"), "invalid_hide_on"),
        }
        let layout = match m.get("layout") {
            None => None,
            Some(Value::String(s)) if layouts(kind).contains(&s.as_str()) => Some(s.as_str()),
            Some(_) => {
                v.err(&join(&p, "layout"), "invalid_layout");
                None
            }
        };
        let x = int_in(v, m, &p, "x", 0, COLS - 1);
        let y = int_in(v, m, &p, "y", 0, MAX_Y);
        let w = int_in(v, m, &p, "w", 1, COLS);
        let h = int_in(v, m, &p, "h", 1, MAX_H);
        if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
            if x + w > COLS {
                v.err(&join(&p, "w"), "out_of_bounds");
            } else if !is_embed(kind, layout) {
                grid.push((i, Rect { x, y, w, h }));
            }
        }
        let dp = join(&p, "data");
        match m.get("data") {
            Some(d) => data(v, kind, d, &dp),
            None => v.err(&dp, "required"),
        }
    }
    // Blok grid tidak boleh tumpang tindih (mesin tata letak editor menjamin ini; server memastikan).
    for (a_idx, (ia, ra)) in grid.iter().enumerate() {
        if grid[..a_idx].iter().any(|(_, rb)| ra.overlaps(rb)) {
            v.err(&format!("blocks[{ia}]"), "overlap");
        }
    }
}

/// Teks alternatif gambar (WCAG 1.1.1) — field v2 di atas data section v1.
const IMAGE_ALT_MAX: usize = 160;

fn data(v: &mut V, kind: &str, d: &Value, p: &str) {
    match kind {
        "text" => {
            if let Some(m) = v.object(d, p, &["title", "body"]) {
                v.text(m, p, "title", 120, false);
                v.text(m, p, "body", 5000, true);
            }
        }
        "chatbot" => {
            if let Some(m) = v.object(d, p, &["title", "welcome"]) {
                v.text(m, p, "title", 40, true);
                v.text(m, p, "welcome", 300, false);
            }
        }
        "whatsapp" => {
            if let Some(m) = v.object(d, p, &["label", "message"]) {
                v.text(m, p, "label", 40, true);
                v.text(m, p, "message", 300, false);
            }
        }
        "hero" | "about" | "promotion" | "gallery" => {
            // Validasi field v2 (alt) lalu serahkan sisanya ke aturan v1 tanpa field tersebut.
            let Value::Object(m) = d else {
                v.err(p, "expected_object");
                return;
            };
            let mut base = m.clone();
            if kind != "gallery" {
                v.text(m, p, "image_alt", IMAGE_ALT_MAX, false);
                base.remove("image_alt");
            } else if let Some(Value::Array(items)) = base.get_mut("images") {
                for (i, it) in items.iter_mut().enumerate() {
                    if let Value::Object(im) = it {
                        v.text(im, &format!("{p}.images[{i}]"), "alt", IMAGE_ALT_MAX, false);
                        im.remove("alt");
                    }
                }
            }
            v.section_data(kind, &Value::Object(base), p);
        }
        _ => v.section_data(kind, d, p),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::website::validate_page;

    fn block(id: &str, kind: &str, x: i64, y: i64, w: i64, h: i64, data: Value) -> Value {
        json!({"id": id, "kind": kind, "x": x, "y": y, "w": w, "h": h, "style": "plain", "auto_height": true, "data": data})
    }

    fn doc(blocks: Vec<Value>) -> Value {
        json!({"schema_version": 2, "template": "modern_store",
               "theme": {"primary": "#C2410C", "font": "poppins", "surface": "cream", "text_scale": 0.9},
               "seo": {"title": "Kopi Senja", "description": "Kedai kopi rumahan di Bandung"},
               "blocks": blocks})
    }

    fn codes(d: &Value) -> Vec<(String, &'static str)> {
        validate_page(d)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.path, e.code))
            .collect()
    }

    #[test]
    fn valid_studio_document_passes_and_collects_assets() {
        let img = Uuid::new_v4();
        let d = doc(vec![
            block(
                "hero-1",
                "hero",
                0,
                0,
                8,
                10,
                json!({"title": "Kopi Senja", "image": format!("asset:{img}"), "image_alt": "Segelas kopi susu"}),
            ),
            {
                let mut b = block(
                    "chat-1",
                    "chatbot",
                    8,
                    0,
                    4,
                    10,
                    json!({"title": "Tanya kami", "welcome": "Halo!"}),
                );
                b["layout"] = json!("inline");
                b["chat_open"] = json!(true);
                b
            },
            block(
                "produk-1",
                "product_grid",
                0,
                10,
                12,
                12,
                json!({"title": "Menu", "limit": 8}),
            ),
            block(
                "teks-1",
                "text",
                0,
                22,
                6,
                4,
                json!({"body": "Buka setiap hari."}),
            ),
            {
                let mut b = block(
                    "wa-1",
                    "whatsapp",
                    8,
                    30,
                    4,
                    3,
                    json!({"label": "Pesan via WhatsApp", "message": "Halo, saya mau pesan"}),
                );
                b["layout"] = json!("floating");
                b
            },
        ]);
        let ok = validate_page(&d).expect("valid");
        assert!(ok.asset_refs.contains(&img));
        assert_eq!(ok.template, "modern_store");
    }

    #[test]
    fn overlap_and_bounds_are_rejected() {
        let d = doc(vec![
            block("a", "text", 0, 0, 8, 4, json!({"body": "x"})),
            block("b", "text", 4, 2, 6, 4, json!({"body": "y"})),
            block("c", "text", 10, 0, 4, 2, json!({"body": "z"})),
        ]);
        let c = codes(&d);
        assert!(c.contains(&("blocks[1]".into(), "overlap")), "{c:?}");
        assert!(
            c.contains(&("blocks[2].w".into(), "out_of_bounds")),
            "{c:?}"
        );
    }

    #[test]
    fn embed_blocks_may_overlap_grid_but_are_limited_to_one() {
        let mut chat = block("chat", "chatbot", 0, 0, 4, 9, json!({"title": "Tanya"}));
        chat["layout"] = json!("bar");
        let mut chat2 = block("chat2", "chatbot", 0, 0, 4, 9, json!({"title": "Tanya"}));
        chat2["layout"] = json!("floating");
        let d = doc(vec![
            block("hero", "hero", 0, 0, 12, 8, json!({"title": "T"})),
            chat,
            chat2,
        ]);
        let c = codes(&d);
        assert!(!c.iter().any(|(_, k)| *k == "overlap"), "{c:?}");
        assert!(
            c.contains(&("blocks[2].kind".into(), "too_many_of_kind")),
            "{c:?}"
        );
    }

    #[test]
    fn closed_objects_enums_and_unsafe_content() {
        let mut b = block(
            "hero",
            "hero",
            0,
            0,
            12,
            8,
            json!({"title": "T", "html": "<script>"}),
        );
        b["layout"] = json!("carousel");
        b["style"] = json!("neon");
        b["onclick"] = json!("x");
        let mut d = doc(vec![b, block("x", "custom_code", 0, 9, 4, 2, json!({}))]);
        d["theme"]["surface"] = json!("pink");
        d["theme"]["text_scale"] = json!(3);
        d["seo"]["title"] = json!("x".repeat(71));
        let c = codes(&d);
        for want in [
            ("blocks[0].data.html", "unknown_field"),
            ("blocks[0].layout", "invalid_layout"),
            ("blocks[0].style", "invalid_style"),
            ("blocks[0].onclick", "unknown_field"),
            ("blocks[1].kind", "unknown_plugin_kind"),
            ("theme.surface", "invalid_surface"),
            ("theme.text_scale", "invalid_text_scale"),
            ("seo.title", "too_long"),
        ] {
            assert!(
                c.contains(&(want.0.into(), want.1)),
                "kurang {want:?} di {c:?}"
            );
        }
    }

    #[test]
    fn v1_rules_still_apply_inside_v2_data() {
        let d = doc(vec![
            block(
                "hero",
                "hero",
                0,
                0,
                12,
                8,
                json!({"title": "T", "cta": {"label": "Klik", "href": "javascript:alert(1)"}, "image": "http://evil/x.png"}),
            ),
            block(
                "galeri",
                "gallery",
                0,
                8,
                12,
                6,
                json!({"images": [{"asset": "asset:not-a-uuid", "alt": "a".repeat(161)}]}),
            ),
        ]);
        let c = codes(&d);
        assert!(
            c.contains(&("blocks[0].data.cta.href".into(), "unsafe_url")),
            "{c:?}"
        );
        assert!(
            c.contains(&("blocks[0].data.image".into(), "invalid_asset_ref")),
            "{c:?}"
        );
        assert!(
            c.contains(&("blocks[1].data.images[0].asset".into(), "invalid_asset_ref")),
            "{c:?}"
        );
        assert!(
            c.contains(&("blocks[1].data.images[0].alt".into(), "too_long")),
            "{c:?}"
        );
    }

    #[test]
    fn chat_open_only_for_chatbot_and_ids_unique() {
        let mut b = block("dup", "text", 0, 0, 6, 2, json!({"body": "x"}));
        b["chat_open"] = json!(true);
        let d = doc(vec![
            b,
            block("dup", "text", 6, 0, 6, 2, json!({"body": "y"})),
        ]);
        let c = codes(&d);
        assert!(
            c.contains(&("blocks[0].chat_open".into(), "unknown_field")),
            "{c:?}"
        );
        assert!(
            c.contains(&("blocks[1].id".into(), "duplicate_id")),
            "{c:?}"
        );
    }

    #[test]
    fn v1_documents_still_validate_through_dispatch() {
        let d = json!({"schema_version": 1, "template": "modern_store",
                       "theme": {"primary": "#000000", "font": "inter"}, "sections": []});
        assert!(validate_page(&d).is_ok());
        let mut v3 = d.clone();
        v3["schema_version"] = json!(3);
        assert!(validate_page(&v3).is_err());
    }

    #[test]
    fn pro_blocks_are_hidden_publicly_without_pro_but_never_deleted_from_source() {
        let src = json!({"schema_version": 2, "template": "t", "theme": {"primary": "#000000", "font": "inter"},
            "blocks": [{"id": "a", "kind": "hero"}, {"id": "b", "kind": "gallery"}, {"id": "c", "kind": "text"}]});
        // Paket tiruan: gallery = pro.
        let plan = |k: &str| if k == "gallery" { "pro" } else { "starter" };
        let mut starter = src.clone();
        assert_eq!(hide_unentitled_blocks(&mut starter, false, plan), 1);
        let ids: Vec<_> = starter["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["a", "c"]);
        let mut pro = src.clone();
        assert_eq!(hide_unentitled_blocks(&mut pro, true, plan), 0);
        assert_eq!(pro, src, "Pro: dokumen utuh");
        // Fase 1: semua plugin resmi starter → tidak ada yang disembunyikan; jenis tak dikenal fail-closed.
        let mut real = src.clone();
        assert_eq!(hide_unentitled_blocks(&mut real, false, plugin_plan), 0);
        assert!(PLUGIN_KINDS.iter().all(|k| plugin_plan(k) == "starter"));
        assert_eq!(plugin_plan("formulir-baru"), "pro");
        // Dokumen v1 tidak disentuh.
        let mut v1 = json!({"schema_version": 1, "sections": [{"type": "gallery"}]});
        assert_eq!(hide_unentitled_blocks(&mut v1, false, plan), 0);
    }
}
