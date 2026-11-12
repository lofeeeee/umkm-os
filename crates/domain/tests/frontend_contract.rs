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
