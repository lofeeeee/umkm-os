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

pub const ROOT_KEYS: [&str; 6] = [
    "schema_version",
    "template",
    "theme",
    "seo",
    "blocks",
    "drawings",
];
pub const THEME_KEYS: [&str; 11] = [
    "primary",
    "font",
    "surface",
    "text_scale",
    "navbar_style",
    "navbar_sticky",
    "background_image",
    "background_size",
    "background_position",
    "background_opacity",
    "hide_badge",
];
pub const NAVBAR_STYLES: [&str; 2] = ["top", "side"];
pub const BACKGROUND_SIZES: [&str; 3] = ["cover", "contain", "repeat"];
pub const MAX_DRAWING_STROKES: usize = 200;
pub const MAX_STROKE_POINTS: usize = 1000;

/// Plugin resmi (ADR-0019). Data 9 jenis pertama memakai aturan section v1.
pub const PLUGIN_KINDS: [&str; 14] = [
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
    "background",
    "image",
];
/// Gaya kotak blok. `soft` = 3D lembut (neumorphism), `flat` = 2D datar tanpa bayangan.
pub const STYLES: [&str; 6] = ["plain", "card", "tint", "dark", "soft", "flat"];
pub const SURFACES: [&str; 3] = ["light", "cream", "dark"];
pub const TEXT_SCALES: [f64; 3] = [0.9, 1.0, 1.1];
pub const HIDE_ON: [&str; 2] = ["mobile", "desktop"];

/// Plugin Latar: cara gambar mengisi kotak. `crop` = selalu penuh, sisi yang lebih dipotong saat lebar layar
/// berubah; `stretch` = gambar utuh ikut melar/menyusut mengikuti kotak.
pub const BACKGROUND_FITS: [&str; 2] = ["crop", "stretch"];
pub const IMAGE_POSITIONS: [&str; 5] = ["center", "top", "bottom", "left", "right"];
/// Foto plugin (Banner/Tentang/Promo/Galeri): penuhi kotak vs tampil utuh.
pub const IMAGE_FITS: [&str; 2] = ["cover", "contain"];

/// Plugin Gambar: `contain` = utuh (cocok untuk PNG transparan/stiker), `crop` = penuhi kotak, `stretch` = melar.
pub const FREE_IMAGE_FITS: [&str; 3] = ["contain", "crop", "stretch"];
/// Pojok layar untuk Gambar mode `popup`.
pub const CORNERS: [&str; 4] = ["bottom-right", "bottom-left", "top-right", "top-left"];

/// Lapisan bebas: boleh menumpuk blok lain (tidak ikut cek tumpang tindih). Latar selalu di belakang;
/// Gambar di depan (`above`), di belakang (`below`), atau menggantung mengikuti gulir (`sticky`).
pub fn is_layer(kind: &str) -> bool {
    matches!(kind, "background" | "image")
}

/// Varian tata letak per plugin; plugin tanpa varian tidak boleh mengirim `layout`.
pub fn layouts(kind: &str) -> &'static [&'static str] {
    match kind {
        "product_grid" => &["grid", "carousel", "list"],
        "chatbot" => &["inline", "floating", "bar"],
        "whatsapp" => &["floating", "inline"],
        "image" => &["above", "below", "sticky", "popup"],
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

/// Lencana "Dibuat dengan UMKM OS" di footer situs: hanya toko Pro yang boleh menyembunyikannya. Seperti
/// `hide_unentitled_blocks`, pilihan di draf/versi tidak diubah (naik paket → langsung berlaku), hanya dibuang
/// dari dokumen publik. Mengembalikan `true` bila pilihan diabaikan.
pub fn enforce_badge(content: &mut Value, has_pro: bool) -> bool {
    if has_pro {
        return false;
    }
    content
        .get_mut("theme")
        .and_then(Value::as_object_mut)
        .and_then(|t| t.remove("hide_badge"))
        .is_some()
}

/// Blok "embed" berada di luar grid (gelembung/bilah) sehingga dikecualikan dari cek tumpang tindih.
pub fn is_embed(kind: &str, layout: Option<&str>) -> bool {
    matches!(
        (kind, layout),
        ("chatbot", Some("floating" | "bar"))
            | ("whatsapp", Some("floating"))
            | ("image", Some("popup"))
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
    let Some(root) = v.object(doc, "", &ROOT_KEYS) else {
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
    match root.get("drawings") {
        None => {}
        Some(Value::Array(items)) => drawings(&mut v, items),
        Some(_) => v.err("drawings", "expected_array"),
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
    let Some(m) = v.object(t, "theme", &THEME_KEYS) else {
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
    match m.get("navbar_style") {
        None => {}
        Some(Value::String(s)) if NAVBAR_STYLES.contains(&s.as_str()) => {}
        Some(_) => v.err("theme.navbar_style", "invalid_navbar_style"),
    }
    v.boolean(m, "theme", "navbar_sticky");
    v.boolean(m, "theme", "hide_badge");
    v.asset(m, "theme", "background_image", false);
    match m.get("background_size") {
        None => {}
        Some(Value::String(s)) if BACKGROUND_SIZES.contains(&s.as_str()) => {}
        Some(_) => v.err("theme.background_size", "invalid_background_size"),
    }
    v.text(m, "theme", "background_position", 40, false);
    match m.get("background_opacity") {
        None => {}
        Some(Value::Number(n)) if n.as_i64().is_some_and(|i| (0..=100).contains(&i)) => {}
        Some(_) => v.err("theme.background_opacity", "out_of_range"),
    }
}

fn is_stroke_color(s: &str) -> bool {
    if s.len() < 3 || s.len() > 30 {
        return false;
    }
    if let Some(rest) = s.strip_prefix('#') {
        return [3, 4, 6, 8].contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_hexdigit());
    }
    if [3, 6, 8].contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = if inner.contains(',') {
            inner.split(',').collect()
        } else {
            inner.split_whitespace().collect()
        };
        if parts.len() != 3 {
            return false;
        }
        return parts.iter().all(|p| {
            let p = p.trim().trim_end_matches('%');
            !p.is_empty() && p.chars().all(|c| c.is_ascii_digit() || c == '.')
        });
    }
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = if inner.contains(',') {
            inner.split(',').collect()
        } else {
            inner.split_whitespace().collect()
        };
        if parts.len() != 4 {
            return false;
        }
        return parts.iter().all(|p| {
            let p = p.trim().trim_end_matches('%');
            !p.is_empty() && p.chars().all(|c| c.is_ascii_digit() || c == '.')
        });
    }
    false
}

fn drawings(v: &mut V, items: &[Value]) {
    if items.len() > MAX_DRAWING_STROKES {
        v.err("drawings", "too_many_drawings");
    }
    for (i, d) in items.iter().take(MAX_DRAWING_STROKES).enumerate() {
        let p = format!("drawings[{i}]");
        let Some(m) = v.object(
            d,
            &p,
            &["id", "target", "color", "size", "opacity", "points"],
        ) else {
            continue;
        };
        match m.get("id") {
            Some(Value::String(s))
                if (1..=40).contains(&s.chars().count()) && !s.chars().any(char::is_control) => {}
            _ => v.err(&join(&p, "id"), "invalid_id"),
        }
        match m.get("target") {
            Some(Value::String(s)) if s == "background" || s.starts_with("cell:") => {}
            _ => v.err(&join(&p, "target"), "invalid_target"),
        }
        match m.get("color") {
            Some(Value::String(s)) if is_stroke_color(s) => {}
            _ => v.err(&join(&p, "color"), "invalid_color"),
        }
        match m.get("size") {
            Some(Value::Number(n)) if n.as_f64().is_some_and(|s| (1.0..=64.0).contains(&s)) => {}
            _ => v.err(&join(&p, "size"), "out_of_range"),
        }
        match m.get("opacity") {
            Some(Value::Number(n)) if n.as_f64().is_some_and(|o| (0.0..=1.0).contains(&o)) => {}
            _ => v.err(&join(&p, "opacity"), "out_of_range"),
        }
        match m.get("points") {
            Some(Value::Array(pts)) => {
                if pts.len() > MAX_STROKE_POINTS {
                    v.err(&join(&p, "points"), "too_many_points");
                }
                for (j, pt) in pts.iter().take(MAX_STROKE_POINTS).enumerate() {
                    let pt_path = format!("{p}.points[{j}]");
                    match pt {
                        Value::Array(coords) if coords.len() == 2 => {
                            let x = coords[0].as_i64();
                            let y = coords[1].as_i64();
                            match (x, y) {
                                (Some(x), Some(y)) if x >= 0 && y >= 0 => {}
                                _ => v.err(&pt_path, "invalid_point"),
                            }
                        }
                        _ => v.err(&pt_path, "invalid_point"),
                    }
                }
            }
            Some(_) => v.err(&join(&p, "points"), "expected_array"),
            None => v.err(&join(&p, "points"), "required"),
        }
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
            } else if !is_embed(kind, layout) && !is_layer(kind) {
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
        "image" => {
            if let Some(m) = v.object(
                d,
                p,
                &[
                    "image", "alt", "fit", "position", "opacity", "href", "corner",
                ],
            ) {
                v.asset(m, p, "image", false);
                v.text(m, p, "alt", IMAGE_ALT_MAX, false);
                enum_opt(v, m, p, "fit", &FREE_IMAGE_FITS);
                enum_opt(v, m, p, "position", &IMAGE_POSITIONS);
                enum_opt(v, m, p, "corner", &CORNERS);
                v.href(m, p, "href", false);
                if m.contains_key("opacity") {
                    v.int(m, p, "opacity", 0, 100);
                }
            }
        }
        "background" => {
            if let Some(m) = v.object(d, p, &["image", "fit", "position", "opacity"]) {
                v.asset(m, p, "image", false);
                enum_opt(v, m, p, "fit", &BACKGROUND_FITS);
                enum_opt(v, m, p, "position", &IMAGE_POSITIONS);
                if m.contains_key("opacity") {
                    v.int(m, p, "opacity", 0, 100);
                }
            }
        }
        "hero" | "about" | "promotion" | "gallery" => {
            // Validasi field v2 (alt) lalu serahkan sisanya ke aturan v1 tanpa field tersebut.
            let Value::Object(m) = d else {
                v.err(p, "expected_object");
                return;
            };
            let mut base = m.clone();
            // Studio: tampilan foto (`image_fit`/`image_position`) — field v2, divalidasi di sini.
            if kind != "gallery" {
                v.text(m, p, "image_alt", IMAGE_ALT_MAX, false);
                enum_opt(v, m, p, "image_fit", &IMAGE_FITS);
                enum_opt(v, m, p, "image_position", &IMAGE_POSITIONS);
                for k in ["image_alt", "image_fit", "image_position"] {
                    base.remove(k);
                }
            } else if let Some(Value::Array(items)) = base.get_mut("images") {
                for (i, it) in items.iter_mut().enumerate() {
                    if let Value::Object(im) = it {
                        let ip = format!("{p}.images[{i}]");
                        v.text(im, &ip, "alt", IMAGE_ALT_MAX, false);
                        enum_opt(v, im, &ip, "image_fit", &IMAGE_FITS);
                        enum_opt(v, im, &ip, "image_position", &IMAGE_POSITIONS);
                        for k in ["alt", "image_fit", "image_position"] {
                            im.remove(k);
                        }
                    }
                }
            }
            v.section_data(kind, &Value::Object(base), p);
        }
        _ => v.section_data(kind, d, p),
    }
}

/// Field string opsional yang nilainya harus salah satu dari `allowed`.
fn enum_opt(v: &mut V, m: &Map<String, Value>, p: &str, key: &str, allowed: &[&str]) {
    match m.get(key) {
        None => {}
        Some(Value::String(s)) if allowed.contains(&s.as_str()) => {}
        Some(_) => v.err(&join(p, key), "invalid_value"),
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
    fn soft_and_flat_styles_are_accepted() {
        let mut a = block("a", "text", 0, 0, 6, 4, json!({"body": "x"}));
        a["style"] = json!("soft");
        let mut b = block("b", "text", 6, 0, 6, 4, json!({"body": "y"}));
        b["style"] = json!("flat");
        let c = codes(&doc(vec![a, b]));
        assert!(!c.iter().any(|(_, k)| *k == "invalid_style"), "{c:?}");
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

    #[test]
    fn background_layer_may_overlap_blocks_and_validates_its_data() {
        let img = Uuid::new_v4();
        let mut bg = block(
            "latar-1",
            "background",
            0,
            0,
            12,
            20,
            json!({"image": format!("asset:{img}"), "fit": "stretch", "position": "top", "opacity": 80}),
        );
        bg["auto_height"] = json!(false);
        let d = doc(vec![
            bg,
            block("a", "text", 0, 0, 6, 4, json!({"body": "x"})),
            block(
                "b",
                "hero",
                6,
                2,
                6,
                8,
                json!({"title": "Hai", "image_fit": "contain", "image_position": "top"}),
            ),
        ]);
        let ok = validate_page(&d).expect("lapisan latar boleh menumpuk blok lain");
        assert!(ok.asset_refs.contains(&img));

        let bad = doc(vec![
            block(
                "latar-1",
                "background",
                0,
                0,
                12,
                4,
                json!({"fit": "tile", "opacity": 101, "x": 1}),
            ),
            block(
                "h",
                "hero",
                0,
                4,
                12,
                4,
                json!({"title": "Hai", "image_fit": "zoom"}),
            ),
        ]);
        let c = codes(&bad);
        assert!(c.contains(&("blocks[0].data.fit".into(), "invalid_value")));
        assert!(c.contains(&("blocks[0].data.opacity".into(), "out_of_range")));
        assert!(c.contains(&("blocks[0].data.x".into(), "unknown_field")));
        assert!(c.contains(&("blocks[1].data.image_fit".into(), "invalid_value")));
    }

    #[test]
    fn free_image_layers_overlap_and_popup_is_outside_the_grid() {
        let img = Uuid::new_v4();
        let mut above = block(
            "gambar-1",
            "image",
            2,
            1,
            4,
            4,
            json!({"image": format!("asset:{img}"), "alt": "Stiker promo", "fit": "contain", "href": "/produk", "opacity": 90}),
        );
        above["layout"] = json!("above");
        let mut sticky = block("gambar-2", "image", 0, 0, 3, 12, json!({"fit": "crop"}));
        sticky["layout"] = json!("sticky");
        let mut popup = block(
            "gambar-3",
            "image",
            0,
            0,
            3,
            3,
            json!({"corner": "bottom-left"}),
        );
        popup["layout"] = json!("popup");
        let d = doc(vec![
            block("a", "text", 0, 0, 12, 6, json!({"body": "x"})),
            above,
            sticky,
            popup,
        ]);
        validate_page(&d).expect("gambar bebas boleh menumpuk blok");

        let mut bad = block(
            "gambar-1",
            "image",
            0,
            0,
            4,
            4,
            json!({"fit": "cover", "corner": "middle", "href": "javascript:alert(1)"}),
        );
        bad["layout"] = json!("floating");
        let c = codes(&doc(vec![bad]));
        assert!(c.contains(&("blocks[0].layout".into(), "invalid_layout")));
        assert!(c.contains(&("blocks[0].data.fit".into(), "invalid_value")));
        assert!(c.contains(&("blocks[0].data.corner".into(), "invalid_value")));
        assert!(c.iter().any(|(p, _)| p == "blocks[0].data.href"));
    }

    #[test]
    fn badge_can_only_be_hidden_on_pro() {
        let src = json!({"schema_version": 2, "theme": {"primary": "#000000", "font": "inter", "hide_badge": true}});
        let mut starter = src.clone();
        assert!(enforce_badge(&mut starter, false));
        assert!(starter["theme"].get("hide_badge").is_none());
        let mut pro = src.clone();
        assert!(!enforce_badge(&mut pro, true));
        assert_eq!(pro["theme"]["hide_badge"], json!(true));
        let mut none = json!({"schema_version": 2});
        assert!(!enforce_badge(&mut none, false));
    }

    #[test]
    fn validates_extended_theme_and_drawings() {
        let bg_asset = Uuid::new_v4();
        let mut d = json!({
            "schema_version": 2,
            "template": "coffee",
            "theme": {
                "primary": "#4F46E5",
                "font": "inter",
                "surface": "cream",
                "text_scale": 1.0,
                "navbar_style": "side",
                "navbar_sticky": true,
                "background_image": format!("asset:{bg_asset}"),
                "background_size": "cover",
                "background_position": "center",
                "background_opacity": 25
            },
            "blocks": [],
            "drawings": [
                {
                    "id": "stroke-1",
                    "target": "background",
                    "color": "#FF0000",
                    "size": 4,
                    "opacity": 0.8,
                    "points": [[10, 20], [30, 40], [50, 60]]
                },
                {
                    "id": "stroke-2",
                    "target": "cell:block-1",
                    "color": "rgb(0, 128, 255)",
                    "size": 2.5,
                    "opacity": 1.0,
                    "points": [[0, 0]]
                }
            ]
        });
        let res = validate_page(&d);
        assert!(res.is_ok(), "extended v2 doc should be valid: {res:?}");
        let valid = res.unwrap();
        assert!(valid.asset_refs.contains(&bg_asset));

        // Invalid navbar style
        d["theme"]["navbar_style"] = json!("invalid");
        let c = codes(&d);
        assert!(
            c.contains(&("theme.navbar_style".into(), "invalid_navbar_style")),
            "{c:?}"
        );

        // Invalid background size
        d["theme"]["navbar_style"] = json!("top");
        d["theme"]["background_size"] = json!("stretched");
        let c = codes(&d);
        assert!(
            c.contains(&("theme.background_size".into(), "invalid_background_size")),
            "{c:?}"
        );
    }

    #[test]
    fn theme_extended_validation_rejections() {
        let mut d = json!({
            "schema_version": 2,
            "template": "coffee",
            "theme": {
                "primary": "#4F46E5",
                "font": "inter",
                "navbar_sticky": "not_a_bool",
                "background_image": "http://example.com/bad.png",
                "background_position": "p".repeat(41),
                "background_opacity": 150,
                "extra_theme_prop": "illegal"
            },
            "blocks": []
        });
        let c = codes(&d);
        assert!(
            c.contains(&("theme.navbar_sticky".into(), "expected_boolean")),
            "{c:?}"
        );
        assert!(
            c.contains(&("theme.background_image".into(), "invalid_asset_ref")),
            "{c:?}"
        );
        assert!(
            c.contains(&("theme.background_position".into(), "too_long")),
            "{c:?}"
        );
        assert!(
            c.contains(&("theme.background_opacity".into(), "out_of_range")),
            "{c:?}"
        );
        assert!(
            c.contains(&("theme.extra_theme_prop".into(), "unknown_field")),
            "{c:?}"
        );

        // Negative opacity
        d["theme"]["background_opacity"] = json!(-5);
        let c = codes(&d);
        assert!(
            c.contains(&("theme.background_opacity".into(), "out_of_range")),
            "{c:?}"
        );

        // Float opacity
        d["theme"]["background_opacity"] = json!(50.5);
        let c = codes(&d);
        assert!(
            c.contains(&("theme.background_opacity".into(), "out_of_range")),
            "{c:?}"
        );
    }

    #[test]
    fn drawings_validation_rejections() {
        // Drawings is not an array
        let mut d = json!({
            "schema_version": 2,
            "template": "coffee",
            "theme": {"primary": "#000000", "font": "inter"},
            "blocks": [],
            "drawings": "not_an_array"
        });
        let c = codes(&d);
        assert!(c.contains(&("drawings".into(), "expected_array")), "{c:?}");

        // Invalid stroke fields
        d["drawings"] = json!([
            {
                "id": "",
                "target": "header",
                "color": "not_a_color",
                "size": 0,
                "opacity": 1.5,
                "points": "not_an_array",
                "unknown_stroke_key": true
            }
        ]);
        let c = codes(&d);
        assert!(
            c.contains(&("drawings[0].id".into(), "invalid_id")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].target".into(), "invalid_target")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].color".into(), "invalid_color")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].size".into(), "out_of_range")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].opacity".into(), "out_of_range")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].points".into(), "expected_array")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].unknown_stroke_key".into(), "unknown_field")),
            "{c:?}"
        );

        // Stroke with points containing negative or invalid coordinate format
        d["drawings"] = json!([
            {
                "id": "stroke-1",
                "target": "background",
                "color": "#fff",
                "size": 10,
                "opacity": 0.5,
                "points": [[-1, 10], [5, -2], [1, 2, 3], "bad_point"]
            }
        ]);
        let c = codes(&d);
        assert!(
            c.contains(&("drawings[0].points[0]".into(), "invalid_point")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].points[1]".into(), "invalid_point")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].points[2]".into(), "invalid_point")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].points[3]".into(), "invalid_point")),
            "{c:?}"
        );

        // Size > 64 and Opacity < 0
        d["drawings"] = json!([
            {
                "id": "stroke-1",
                "target": "cell:x",
                "color": "#ffffff",
                "size": 65,
                "opacity": -0.1,
                "points": []
            }
        ]);
        let c = codes(&d);
        assert!(
            c.contains(&("drawings[0].size".into(), "out_of_range")),
            "{c:?}"
        );
        assert!(
            c.contains(&("drawings[0].opacity".into(), "out_of_range")),
            "{c:?}"
        );

        // Stroke id too long
        d["drawings"] = json!([
            {
                "id": "x".repeat(41),
                "target": "background",
                "color": "#ffffff",
                "size": 10,
                "opacity": 0.5,
                "points": []
            }
        ]);
        let c = codes(&d);
        assert!(
            c.contains(&("drawings[0].id".into(), "invalid_id")),
            "{c:?}"
        );
    }

    #[test]
    fn drawings_and_points_length_limits() {
        // Exceeding MAX_DRAWING_STROKES (200)
        let strokes: Vec<Value> = (0..201)
            .map(|i| {
                json!({
                    "id": format!("s-{i}"),
                    "target": "background",
                    "color": "#000",
                    "size": 1,
                    "opacity": 1.0,
                    "points": []
                })
            })
            .collect();
        let d = json!({
            "schema_version": 2,
            "template": "coffee",
            "theme": {"primary": "#000000", "font": "inter"},
            "blocks": [],
            "drawings": strokes
        });
        let c = codes(&d);
        assert!(
            c.contains(&("drawings".into(), "too_many_drawings")),
            "{c:?}"
        );

        // Exceeding MAX_STROKE_POINTS (1000)
        let pts: Vec<[i64; 2]> = (0..1001).map(|i| [i, i]).collect();
        let d2 = json!({
            "schema_version": 2,
            "template": "coffee",
            "theme": {"primary": "#000000", "font": "inter"},
            "blocks": [],
            "drawings": [
                {
                    "id": "s-1",
                    "target": "background",
                    "color": "#000",
                    "size": 1,
                    "opacity": 1.0,
                    "points": pts
                }
            ]
        });
        let c2 = codes(&d2);
        assert!(
            c2.contains(&("drawings[0].points".into(), "too_many_points")),
            "{c2:?}"
        );
    }

    #[test]
    fn drawings_color_format_variations() {
        let valid_colors = [
            "#fff",
            "#FFF",
            "#ffff",
            "#ffffff",
            "#ffffffff",
            "fff",
            "ffffff",
            "ffffffff",
            "rgb(0, 0, 0)",
            "rgb(255, 128, 0)",
            "rgba(0, 0, 0, 0)",
            "rgba(255, 255, 255, 0.85)",
            "rgb(100%, 50%, 0%)",
        ];
        for col in valid_colors {
            let d = json!({
                "schema_version": 2,
                "template": "coffee",
                "theme": {"primary": "#000000", "font": "inter"},
                "blocks": [],
                "drawings": [
                    {
                        "id": "s-1",
                        "target": "background",
                        "color": col,
                        "size": 5,
                        "opacity": 0.5,
                        "points": [[0, 0]]
                    }
                ]
            });
            assert!(validate_page(&d).is_ok(), "color '{col}' should be valid");
        }

        let invalid_colors = [
            "",
            "#",
            "#12",
            "#12345",
            "#1234567",
            "#123456789",
            "blue",
            "rgb()",
            "rgb(alert(1))",
            "rgba(1,2)",
            "rgba(0,0,0,1);",
            "not-a-color-at-all",
        ];
        for col in invalid_colors {
            let d = json!({
                "schema_version": 2,
                "template": "coffee",
                "theme": {"primary": "#000000", "font": "inter"},
                "blocks": [],
                "drawings": [
                    {
                        "id": "s-1",
                        "target": "background",
                        "color": col,
                        "size": 5,
                        "opacity": 0.5,
                        "points": [[0, 0]]
                    }
                ]
            });
            let c = codes(&d);
            assert!(
                c.contains(&("drawings[0].color".into(), "invalid_color")),
                "color '{col}' should be rejected as invalid_color, got {c:?}"
            );
        }
    }
}
