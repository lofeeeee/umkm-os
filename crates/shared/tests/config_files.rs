//! P0-2: file `config/*.toml` asli dimuat oleh loader dan tidak berisi secret.

use std::path::PathBuf;

use umkm_shared::config::{Environment, ROOT_DOMAIN_PLACEHOLDER, Settings};

fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config")
}

#[test]
fn development_and_staging_load() {
    for env in [Environment::Development, Environment::Staging] {
        let s = Settings::load(&config_dir(), env).unwrap_or_else(|e| panic!("{env}: {e}"));
        assert_eq!(s.environment, env);
        assert_eq!(s.redis.key_prefix, format!("umkm:{env}"));
    }
}

#[test]
fn production_uses_d01_root_domain() {
    // ADR-0001 (D-01 DISETUJUI 2026-11-11).
    let s = Settings::load(&config_dir(), Environment::Production).unwrap();
    assert_eq!(s.tenancy.root_domain, "ryasai.my.id");
    assert_ne!(s.tenancy.root_domain, ROOT_DOMAIN_PLACEHOLDER);
}

/// URL dengan kredensial (`scheme://user:pass@host`) = secret.
fn has_url_credentials(line: &str) -> bool {
    line.split("://").skip(1).any(|rest| {
        let authority = rest
            .split(['/', '"', '\'', ' ', ',', ']'])
            .next()
            .unwrap_or("");
        authority.contains('@')
    })
}

#[test]
fn config_files_contain_no_secrets() {
    let forbidden = ["password", "secret", "api_key", "apikey", "token"];
    for entry in std::fs::read_dir(config_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "toml") {
            let raw = std::fs::read_to_string(&path).unwrap().to_lowercase();
            for line in raw.lines().filter(|l| !l.trim_start().starts_with('#')) {
                for f in forbidden {
                    assert!(
                        !line.contains(f),
                        "{}: baris berisi {f:?}: {line}",
                        path.display()
                    );
                }
                assert!(
                    !has_url_credentials(line),
                    "{}: URL berkredensial: {line}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn url_credential_detector() {
    assert!(has_url_credentials(r#"url = "postgres://u:p@db/x""#));
    assert!(has_url_credentials(r#"x = ["redis://:pw@redis:6379"]"#));
    assert!(!has_url_credentials(
        r#"cors = ["https://app.ryasai.my.id"]"#
    ));
    assert!(!has_url_credentials(r#"x = "https://h/path@not-userinfo""#));
}
