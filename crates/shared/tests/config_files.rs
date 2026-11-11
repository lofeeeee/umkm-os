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
fn production_blocked_until_d01_decided() {
    // Ketika D-01 DISETUJUI dan root_domain diisi, ubah tes ini menjadi "loads".
    let raw = std::fs::read_to_string(config_dir().join("production.toml")).unwrap();
    assert!(raw.contains(ROOT_DOMAIN_PLACEHOLDER));
    let err = Settings::load(&config_dir(), Environment::Production).unwrap_err();
    assert!(err.to_string().contains("D-01"), "{err}");
}

#[test]
fn config_files_contain_no_secrets() {
    let forbidden = ["password", "secret", "api_key", "apikey", "token", "://"];
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
            }
        }
    }
}
