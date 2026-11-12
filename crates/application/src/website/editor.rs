use std::collections::BTreeSet;

use serde_json::Value;
use umkm_domain::website::{ValidPage, validate_page};
use uuid::Uuid;

pub type Errors = Vec<(String, &'static str)>;

/// Tahap 1 (tanpa I/O): validasi skema + template harus sama dengan template website.
pub fn check_document(doc: &Value, website_template: &str) -> Result<ValidPage, Errors> {
    let v = validate_page(doc)
        .map_err(|e| e.into_iter().map(|e| (e.path, e.code)).collect::<Errors>())?;
    if v.template != website_template {
        return Err(vec![("template".into(), "template_mismatch")]);
    }
    Ok(v)
}

/// Tahap 2: rujukan aset yang tidak ada / bukan milik tenant -> error per rujukan.
pub fn check_assets(v: &ValidPage, owned: &BTreeSet<Uuid>) -> Result<(), Errors> {
    let missing: Errors = v
        .asset_refs
        .iter()
        .filter(|id| !owned.contains(id))
        .map(|id| (format!("asset:{id}"), "asset_not_found"))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn doc(template: &str, asset: Uuid) -> Value {
        json!({"schema_version": 1, "template": template, "theme": {"primary": "#000000", "font": "inter"},
               "sections": [{"id": "hero-1", "type": "hero", "enabled": true,
                             "data": {"title": "Hi", "image": format!("asset:{asset}")}}]})
    }

    #[test]
    fn template_must_match_website() {
        let a = Uuid::new_v4();
        assert!(check_document(&doc("modern_store", a), "modern_store").is_ok());
        assert_eq!(
            check_document(&doc("warung_hangat", a), "modern_store").unwrap_err(),
            vec![("template".to_owned(), "template_mismatch")]
        );
    }

    #[test]
    fn foreign_or_missing_assets_are_reported() {
        let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
        let v = check_document(&doc("modern_store", other), "modern_store").unwrap();
        assert!(check_assets(&v, &BTreeSet::from([other])).is_ok());
        assert_eq!(
            check_assets(&v, &BTreeSet::from([mine])).unwrap_err(),
            vec![(format!("asset:{other}"), "asset_not_found")]
        );
    }
}
