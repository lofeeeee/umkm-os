//! Use case registrasi bisnis (P1-3, `PRD.md §4.2`, `PRD.md §13.3` Registration).
//!
//! Satu transaksi: user + tenant + membership Owner + subdomain utama + business
//! profile + persetujuan legal (D-12) + sesi login. Penyimpanan, hashing, dan
//! pembangkitan token adalah port; implementasi ada di `infrastructure`.

use std::{future::Future, time::Duration};

use umkm_domain::{
    auth::{AuthError, AuthPolicy, Email, TokenHash, validate_password},
    tenant::{Slug, TenantError, slugify, validate_business_name},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

/// Jumlah kandidat slug otomatis sebelum memakai sufiks acak.
const AUTO_SUFFIXES: u32 = 5;

#[derive(Debug, Clone)]
pub struct RegisterCommand {
    pub email: String,
    pub password: String,
    pub business_name: String,
    /// `None` = slug otomatis dari nama bisnis (TEN-004).
    pub slug: Option<String>,
    pub accept_terms: bool,
    pub accept_privacy: bool,
    pub user_agent: Option<String>,
}

/// Versi dokumen legal yang sedang berlaku (D-12; placeholder sampai teks final).
#[derive(Debug, Clone)]
pub struct LegalVersions {
    pub privacy_policy: String,
    pub terms_of_service: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegisterError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Tenant(#[from] TenantError),
    #[error("persetujuan syarat & kebijakan privasi wajib")]
    ConsentRequired,
    #[error("email sudah terdaftar")]
    EmailTaken,
    #[error("kesalahan internal")]
    Internal,
}

/// Data siap simpan; semua sudah tervalidasi.
#[derive(Debug, Clone)]
pub struct NewRegistration {
    pub tenant_id: TenantId,
    pub email: Email,
    pub password_hash: String,
    pub business_name: String,
    /// Dicoba berurutan; yang pertama tersedia dipakai.
    pub slug_candidates: Vec<Slug>,
    pub root_domain: String,
    pub legal: LegalVersions,
    pub session_token_hash: TokenHash,
    pub session_ttl: Duration,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    pub user_id: Uuid,
    pub tenant_id: TenantId,
    pub slug: Slug,
    pub hostname: String,
    pub session_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("email sudah terdaftar")]
    EmailTaken,
    #[error("semua kandidat slug tidak tersedia")]
    SlugUnavailable,
    #[error("storage: {0}")]
    Other(String),
}

pub trait RegistrationStore: Send + Sync {
    fn create(
        &self,
        reg: NewRegistration,
    ) -> impl Future<Output = Result<Registered, StoreError>> + Send;
}

/// Hashing password yang tidak memblokir runtime async (Argon2id mahal).
pub trait PasswordHashing: Send + Sync {
    fn hash(&self, password: String) -> impl Future<Output = Result<String, AuthError>> + Send;
}

pub trait TokenIssuer: Send + Sync {
    /// Token opak plaintext + hash untuk disimpan.
    fn issue(&self) -> Result<(String, TokenHash), AuthError>;
}

#[derive(Debug, Clone)]
pub struct RegistrationOutcome {
    pub registered: Registered,
    /// Plaintext token sesi; hanya untuk cookie, tidak pernah disimpan.
    pub session_token: String,
}

/// Kandidat slug: eksplisit -> hanya itu; otomatis -> base, base-2..base-N, base-{acak}.
pub fn slug_candidates(
    requested: Option<&str>,
    business_name: &str,
    tenant_id: TenantId,
) -> Result<Vec<Slug>, TenantError> {
    if let Some(raw) = requested {
        return Ok(vec![Slug::parse(raw)?]);
    }
    let base = slugify(business_name);
    let mut out = vec![base.clone()];
    out.extend((2..=AUTO_SUFFIXES + 1).map(|n| base.with_suffix(n)));
    // Sufiks dari tenant_id (acak v4) sebagai kandidat terakhir.
    let tail = &tenant_id.as_uuid().simple().to_string()[..6];
    let suffix = format!("-{tail}");
    let keep = umkm_domain::tenant::SLUG_MAX - suffix.len();
    let head = &base.as_str()[..base.as_str().len().min(keep)];
    out.push(Slug::parse(&format!(
        "{}{suffix}",
        head.trim_end_matches('-')
    ))?);
    Ok(out)
}

pub async fn register(
    store: &impl RegistrationStore,
    hasher: &impl PasswordHashing,
    tokens: &impl TokenIssuer,
    cmd: RegisterCommand,
    root_domain: &str,
    policy: &AuthPolicy,
    legal: &LegalVersions,
) -> Result<RegistrationOutcome, RegisterError> {
    if !(cmd.accept_terms && cmd.accept_privacy) {
        return Err(RegisterError::ConsentRequired);
    }
    let email = Email::parse(&cmd.email)?;
    validate_password(&cmd.password, &email)?;
    let business_name = validate_business_name(&cmd.business_name)?;
    let tenant_id = TenantId::new_v4();
    let slug_candidates = slug_candidates(cmd.slug.as_deref(), &business_name, tenant_id)?;

    let password_hash = hasher.hash(cmd.password).await?;
    let (session_token, session_token_hash) = tokens.issue()?;

    let registered = store
        .create(NewRegistration {
            tenant_id,
            email,
            password_hash,
            business_name,
            slug_candidates,
            root_domain: root_domain.to_owned(),
            legal: legal.clone(),
            session_token_hash,
            session_ttl: policy.session_ttl,
            user_agent: cmd.user_agent,
        })
        .await
        .map_err(|e| match e {
            StoreError::EmailTaken => RegisterError::EmailTaken,
            StoreError::SlugUnavailable => RegisterError::Tenant(TenantError::SlugUnavailable),
            StoreError::Other(_) => RegisterError::Internal,
        })?;
    Ok(RegistrationOutcome {
        registered,
        session_token,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct FakeStore(Mutex<Option<NewRegistration>>, Option<StoreError>);
    impl RegistrationStore for FakeStore {
        async fn create(&self, reg: NewRegistration) -> Result<Registered, StoreError> {
            if let Some(e) = &self.1 {
                return Err(e.clone());
            }
            let out = Registered {
                user_id: Uuid::nil(),
                tenant_id: reg.tenant_id,
                slug: reg.slug_candidates[0].clone(),
                hostname: reg.slug_candidates[0].hostname(&reg.root_domain),
                session_id: Uuid::nil(),
            };
            *self.0.lock().unwrap() = Some(reg);
            Ok(out)
        }
    }
    struct FakeHasher;
    impl PasswordHashing for FakeHasher {
        async fn hash(&self, p: String) -> Result<String, AuthError> {
            Ok(format!("$argon2id$fake${}", p.len()))
        }
    }
    struct FakeTokens;
    impl TokenIssuer for FakeTokens {
        fn issue(&self) -> Result<(String, TokenHash), AuthError> {
            Ok(("plain".into(), TokenHash([9; 32])))
        }
    }

    fn cmd() -> RegisterCommand {
        RegisterCommand {
            email: " Budi@X.id ".into(),
            password: "rahasia-123".into(),
            business_name: "Toko Budi".into(),
            slug: None,
            accept_terms: true,
            accept_privacy: true,
            user_agent: None,
        }
    }

    fn legal() -> LegalVersions {
        LegalVersions {
            privacy_policy: "placeholder-v0".into(),
            terms_of_service: "placeholder-v0".into(),
        }
    }

    async fn run(
        c: RegisterCommand,
        store: &FakeStore,
    ) -> Result<RegistrationOutcome, RegisterError> {
        register(
            store,
            &FakeHasher,
            &FakeTokens,
            c,
            "ryasai.my.id",
            &AuthPolicy::default(),
            &legal(),
        )
        .await
    }

    #[tokio::test]
    async fn happy_path_passes_validated_data_and_never_plain_password() {
        let store = FakeStore(Mutex::new(None), None);
        let out = run(cmd(), &store).await.unwrap();
        assert_eq!(out.registered.hostname, "toko-budi.ryasai.my.id");
        assert_eq!(out.session_token, "plain");
        let saved = store.0.lock().unwrap().clone().unwrap();
        assert_eq!(saved.email.as_str(), "Budi@X.id");
        assert!(!saved.password_hash.contains("rahasia"));
        assert_eq!(saved.slug_candidates[0].as_str(), "toko-budi");
        assert_eq!(saved.slug_candidates[1].as_str(), "toko-budi-2");
        assert_eq!(saved.slug_candidates.len(), 7);
        assert_eq!(saved.session_token_hash, TokenHash([9; 32]));
    }

    #[tokio::test]
    async fn consent_is_required() {
        let store = FakeStore(Mutex::new(None), None);
        for (t, p) in [(false, true), (true, false), (false, false)] {
            let c = RegisterCommand {
                accept_terms: t,
                accept_privacy: p,
                ..cmd()
            };
            assert_eq!(
                run(c, &store).await.unwrap_err(),
                RegisterError::ConsentRequired
            );
        }
        assert!(store.0.lock().unwrap().is_none(), "tidak ada yang disimpan");
    }

    #[tokio::test]
    async fn validation_errors_before_any_storage() {
        let store = FakeStore(Mutex::new(None), None);
        assert!(matches!(
            run(
                RegisterCommand {
                    email: "x".into(),
                    ..cmd()
                },
                &store
            )
            .await,
            Err(RegisterError::Auth(AuthError::InvalidEmail))
        ));
        assert!(matches!(
            run(
                RegisterCommand {
                    password: "short".into(),
                    ..cmd()
                },
                &store
            )
            .await,
            Err(RegisterError::Auth(AuthError::WeakPassword(_)))
        ));
        assert!(matches!(
            run(
                RegisterCommand {
                    business_name: " ".into(),
                    ..cmd()
                },
                &store
            )
            .await,
            Err(RegisterError::Tenant(TenantError::InvalidName))
        ));
        assert!(matches!(
            run(
                RegisterCommand {
                    slug: Some("Bad Slug".into()),
                    ..cmd()
                },
                &store
            )
            .await,
            Err(RegisterError::Tenant(TenantError::InvalidSlug))
        ));
        assert!(store.0.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn explicit_slug_is_the_only_candidate() {
        let store = FakeStore(Mutex::new(None), None);
        run(
            RegisterCommand {
                slug: Some("warung-siti".into()),
                ..cmd()
            },
            &store,
        )
        .await
        .unwrap();
        let saved = store.0.lock().unwrap().clone().unwrap();
        assert_eq!(
            saved.slug_candidates,
            vec![Slug::parse("warung-siti").unwrap()]
        );
    }

    #[tokio::test]
    async fn store_errors_are_mapped() {
        for (e, want) in [
            (StoreError::EmailTaken, RegisterError::EmailTaken),
            (
                StoreError::SlugUnavailable,
                RegisterError::Tenant(TenantError::SlugUnavailable),
            ),
            (StoreError::Other("boom".into()), RegisterError::Internal),
        ] {
            let store = FakeStore(Mutex::new(None), Some(e));
            assert_eq!(run(cmd(), &store).await.unwrap_err(), want);
        }
    }

    #[test]
    fn random_tail_candidate_is_valid_for_long_names() {
        let c = slug_candidates(None, &"x".repeat(200), TenantId::new_v4()).unwrap();
        assert!(c.iter().all(|s| s.as_str().len() <= 63));
        let last = c.last().unwrap().as_str();
        assert_eq!(last.len(), 63);
    }
}
