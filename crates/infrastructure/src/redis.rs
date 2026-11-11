//! Namespacing key Redis (P0-5, `PRD.md §6.3`, `PRD.md §9.5`).
//!
//! Format:
//! ```text
//! {prefix}:tenant:{tenant_id}:{resource}:{id}   data milik tenant
//! {prefix}:platform:{resource}:{id}             data platform (bukan milik tenant)
//! ```
//! `prefix` = `redis.key_prefix` per environment (mis. `umkm:production`).
//!
//! Setiap segmen dibatasi ke `[A-Za-z0-9_.-]`, tidak kosong, maks 128 byte. Karena
//! segmen tidak dapat memuat `:`, pemetaan (tenant, resource, id) -> key bersifat
//! injektif: dua tenant tidak mungkin menghasilkan key yang sama. Karakter glob
//! (`* ? [ ]`) juga ditolak sehingga pola SCAN tidak dapat melintasi tenant.
//!
//! Redis bukan source of truth untuk order/payment/inventory/ticket/approval/audit.

use std::fmt;

use umkm_shared::ids::TenantId;

pub const MAX_SEGMENT_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    #[error("segmen key kosong")]
    Empty,
    #[error("segmen key melebihi {MAX_SEGMENT_LEN} byte")]
    TooLong,
    #[error("segmen key memuat karakter terlarang (hanya A-Z a-z 0-9 _ . -)")]
    InvalidChar,
}

fn validate_segment(s: &str) -> Result<(), KeyError> {
    if s.is_empty() {
        return Err(KeyError::Empty);
    }
    if s.len() > MAX_SEGMENT_LEN {
        return Err(KeyError::TooLong);
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    {
        return Err(KeyError::InvalidChar);
    }
    Ok(())
}

/// Key Redis yang sudah tervalidasi dan ter-namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RedisKey(String);

impl RedisKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RedisKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Namespace per environment. Prefix boleh terdiri dari beberapa segmen dipisah `:`.
#[derive(Debug, Clone)]
pub struct KeyNamespace {
    prefix: String,
}

impl KeyNamespace {
    pub fn new(prefix: &str) -> Result<Self, KeyError> {
        prefix.split(':').try_for_each(validate_segment)?;
        Ok(Self {
            prefix: prefix.to_owned(),
        })
    }

    pub fn tenant(&self, tenant: TenantId) -> TenantKeys<'_> {
        TenantKeys { ns: self, tenant }
    }

    pub fn platform(&self) -> PlatformKeys<'_> {
        PlatformKeys { ns: self }
    }
}

/// Builder key untuk satu tenant. Tidak ada cara membuat key tenant tanpa `TenantId`.
#[derive(Debug, Clone, Copy)]
pub struct TenantKeys<'a> {
    ns: &'a KeyNamespace,
    tenant: TenantId,
}

impl TenantKeys<'_> {
    /// `{prefix}:tenant:{tenant}:{resource}:{id}`
    pub fn key(&self, resource: &str, id: &str) -> Result<RedisKey, KeyError> {
        validate_segment(resource)?;
        validate_segment(id)?;
        Ok(RedisKey(format!(
            "{}:tenant:{}:{resource}:{id}",
            self.ns.prefix, self.tenant
        )))
    }

    /// Pola SCAN yang hanya mencakup key tenant ini: `{prefix}:tenant:{tenant}:*`.
    pub fn scan_pattern(&self) -> String {
        format!("{}:tenant:{}:*", self.ns.prefix, self.tenant)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlatformKeys<'a> {
    ns: &'a KeyNamespace,
}

impl PlatformKeys<'_> {
    /// `{prefix}:platform:{resource}:{id}`
    pub fn key(&self, resource: &str, id: &str) -> Result<RedisKey, KeyError> {
        validate_segment(resource)?;
        validate_segment(id)?;
        Ok(RedisKey(format!(
            "{}:platform:{resource}:{id}",
            self.ns.prefix
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn ns() -> KeyNamespace {
        KeyNamespace::new("umkm:test").unwrap()
    }

    fn tenant(n: u128) -> TenantId {
        TenantId::from_uuid(uuid::Uuid::from_u128(n))
    }

    #[test]
    fn tenant_key_format_matches_prd() {
        let t = tenant(1);
        let k = ns().tenant(t).key("products", "p-1").unwrap();
        assert_eq!(k.as_str(), format!("umkm:test:tenant:{t}:products:p-1"));
    }

    #[test]
    fn platform_key_format() {
        let k = ns().platform().key("rate_limit", "ip-1").unwrap();
        assert_eq!(k.as_str(), "umkm:test:platform:rate_limit:ip-1");
    }

    #[test]
    fn rejects_segments_that_could_break_namespace() {
        let ns = ns();
        let t = ns.tenant(tenant(1));
        for bad in [
            "", "a:b", "*", "a*", "a?", "[a]", "a b", "a\nb", "ü", "a/b", "a{b}",
        ] {
            assert!(t.key("products", bad).is_err(), "id {bad:?} harus ditolak");
            assert!(t.key(bad, "x").is_err(), "resource {bad:?} harus ditolak");
            assert!(ns.platform().key(bad, "x").is_err());
        }
        assert_eq!(t.key("p", &"a".repeat(129)), Err(KeyError::TooLong));
        assert!(t.key("p", &"a".repeat(128)).is_ok());
    }

    #[test]
    fn rejects_invalid_prefix() {
        for bad in ["", "umkm:", ":umkm", "umkm::x", "umkm *", "umkm:*"] {
            assert!(
                KeyNamespace::new(bad).is_err(),
                "prefix {bad:?} harus ditolak"
            );
        }
    }

    #[test]
    fn no_collisions_across_tenants_resources_ids() {
        let ns = ns();
        let ids = ["1", "a", "a-b", "a.b", "a_b", "ab", "b"];
        let resources = ["products", "conversation", "inventory", "products-x"];
        let mut seen = HashSet::new();
        let mut count = 0;
        for t in 1..=20 {
            for r in resources {
                for id in ids {
                    count += 1;
                    assert!(
                        seen.insert(ns.tenant(tenant(t)).key(r, id).unwrap()),
                        "collision: tenant {t} {r} {id}"
                    );
                }
            }
        }
        for r in resources {
            for id in ids {
                count += 1;
                assert!(seen.insert(ns.platform().key(r, id).unwrap()));
            }
        }
        assert_eq!(seen.len(), count);
    }

    #[test]
    fn tenant_scan_pattern_never_matches_other_tenant_keys() {
        let ns = ns();
        let (a, b) = (ns.tenant(tenant(1)), ns.tenant(tenant(2)));
        let prefix_a = a.scan_pattern().trim_end_matches('*').to_owned();
        for k in [
            b.key("products", "x").unwrap(),
            ns.platform().key("products", "x").unwrap(),
        ] {
            assert!(
                !k.as_str().starts_with(&prefix_a),
                "{k} cocok dengan pola A"
            );
        }
        assert!(
            a.key("products", "x")
                .unwrap()
                .as_str()
                .starts_with(&prefix_a)
        );
    }

    #[test]
    fn environments_do_not_collide() {
        let t = tenant(1);
        let dev = KeyNamespace::new("umkm:development").unwrap();
        let prod = KeyNamespace::new("umkm:production").unwrap();
        assert_ne!(
            dev.tenant(t).key("products", "x").unwrap(),
            prod.tenant(t).key("products", "x").unwrap()
        );
    }

    #[test]
    fn config_prefixes_are_valid() {
        for env in ["development", "staging", "production"] {
            let raw = std::fs::read_to_string(format!(
                "{}/../../config/{env}.toml",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            let settings = umkm_shared::config::Settings::parse(&raw).unwrap();
            KeyNamespace::new(&settings.redis.key_prefix).unwrap_or_else(|e| panic!("{env}: {e}"));
        }
    }
}
