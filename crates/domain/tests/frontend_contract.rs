//! Kontrak lintas bahasa: default section yang dibuat editor dashboard (frontend) harus
//! lolos validator Rust (ADR-0007). Bila salah satu berubah tanpa yang lain, tes ini gagal.

use serde_json::{Value, json};
use umkm_domain::website::{schema::SECTION_TYPES, validate_page};

const DEFAULTS: &str = include_str!("../../../frontend/src/renderer/section-defaults.json");

#[test]
fn every_editor_default_section_validates() {
    let defaults: Value = serde_json::from_str(DEFAULTS).expect("JSON valid");
    let map = defaults.as_object().expect("objek");
    let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut want = SECTION_TYPES.to_vec();
    want.sort_unstable();
    assert_eq!(
        keys, want,
        "frontend harus punya default untuk tepat 11 tipe section"
    );
    let sections: Vec<Value> = SECTION_TYPES
        .iter()
        .map(|t| json!({"id": format!("{}-1", t.replace('_', "-")), "type": t, "enabled": true, "data": map[*t]}))
        .collect();
    let doc = json!({"schema_version": 1, "template": "modern_store",
                     "theme": {"primary": "#2563EB", "font": "inter"}, "sections": sections});
    if let Err(e) = validate_page(&doc) {
        panic!("default editor tidak valid: {e:?}");
    }
}

/// ADR-0019: manifest plugin frontend = daftar plugin & varian tata letak yang diterima validator v2.
#[test]
fn studio_plugin_manifest_matches_validator() {
    use umkm_domain::website::schema_v2::{PLUGIN_KINDS, is_embed, layouts, max_per_page};
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../frontend/src/studio/plugin-manifest.json"
    ))
    .expect("JSON");
    let plugins = manifest["plugins"].as_object().expect("plugins");
    let mut fe: Vec<&str> = plugins.keys().map(String::as_str).collect();
    fe.sort_unstable();
    let mut be = PLUGIN_KINDS.to_vec();
    be.sort_unstable();
    assert_eq!(fe, be, "jenis plugin frontend ≠ validator");
    for (kind, def) in plugins {
        let fl: Vec<&str> = def["layouts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        assert_eq!(fl, layouts(kind), "layout {kind}");
        assert_eq!(
            def["max_per_page"].as_u64().unwrap() as usize,
            max_per_page(kind),
            "max_per_page {kind}"
        );
        assert!(
            matches!(def["plan"].as_str(), Some("starter" | "pro")),
            "plan {kind}"
        );
    }
    for (kind, ls) in manifest["embed_layouts"].as_object().unwrap() {
        for l in ls.as_array().unwrap() {
            assert!(is_embed(kind, l.as_str()), "embed {kind}/{l}");
        }
    }
}

/// Dokumen Studio dari default section + plugin baru lolos validator v2 (kanvas bertumpuk).
#[test]
fn studio_document_from_defaults_validates() {
    let defaults: Value = serde_json::from_str(DEFAULTS).expect("JSON valid");
    let map = defaults.as_object().unwrap();
    let mut blocks = Vec::new();
    let mut y = 0;
    for kind in umkm_domain::website::schema_v2::PLUGIN_KINDS {
        let data = match kind {
            "text" => json!({"body": "Teks"}),
            "chatbot" => json!({"title": "Tanya kami", "welcome": "Halo!"}),
            "whatsapp" => json!({"label": "Pesan via WhatsApp"}),
            k => map[k].clone(),
        };
        blocks.push(json!({"id": format!("{}-1", kind.replace('_', "-")), "kind": kind, "x": 0, "y": y, "w": 12, "h": 4, "style": "plain", "data": data}));
        y += 4;
    }
    let doc = json!({"schema_version": 2, "template": "modern_store", "theme": {"primary": "#2563EB", "font": "inter"}, "blocks": blocks});
    if let Err(e) = validate_page(&doc) {
        panic!("dokumen Studio dari default tidak valid: {e:?}");
    }
}

/// Setiap template Studio (frontend/src/studio/templates.ts → toDoc) lolos validator v2.
#[test]
fn every_studio_template_validates() {
    let all: Value =
        serde_json::from_str(include_str!("fixtures/studio_templates.json")).expect("JSON");
    let map = all.as_object().expect("objek");
    assert!(map.len() >= 6, "fixture template kosong");
    for (key, doc) in map {
        if let Err(e) = validate_page(doc) {
            panic!("template {key} ditolak validator: {e:?}");
        }
    }
}

/// Dokumen nyata yang dikirim browser Studio (tinggi hasil pengukuran, P9-3) lolos validator v2.
#[test]
fn studio_browser_document_validates() {
    let doc: Value =
        serde_json::from_str(include_str!("fixtures/studio_browser_doc.json")).expect("JSON");
    if let Err(e) = validate_page(&doc) {
        panic!("dokumen dari browser ditolak: {e:?}");
    }
}
