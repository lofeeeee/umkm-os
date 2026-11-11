//! Event outbox dan port pengiriman email (P1-6, `PRD.md §4.13`).
//!
//! Provider email belum diputuskan (D-08 ditunda): port ini provider-neutral.

use std::future::Future;

use uuid::Uuid;

/// Tipe event yang dikenal worker. Payload tidak pernah berisi secret.
pub mod events {
    pub const EMAIL_VERIFICATION_REQUESTED: &str = "auth.email_verification_requested";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEvent {
    pub tenant_id: Option<Uuid>,
    pub event_type: &'static str,
    pub payload: serde_json::Value,
    /// Unik global; enqueue ulang dengan kunci sama diabaikan (BR-017).
    pub idempotency_key: String,
}

impl OutboxEvent {
    pub fn email_verification_requested(user_id: Uuid) -> Self {
        Self {
            tenant_id: None,
            event_type: events::EMAIL_VERIFICATION_REQUESTED,
            payload: serde_json::json!({ "user_id": user_id }),
            idempotency_key: format!("email_verification:{user_id}:registration"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailMessage {
    pub to: String,
    /// Nama template stabil (untuk log/metrics; isi tidak di-log).
    pub template: &'static str,
    pub subject: String,
    pub text_body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("pengiriman email gagal ({class}); retryable={retryable}")]
pub struct SendError {
    pub retryable: bool,
    /// Kelas error tanpa data pribadi, mis. `timeout`, `rejected`, `rate_limited`.
    pub class: &'static str,
}

pub trait EmailSender: Send + Sync {
    fn send(&self, msg: &EmailMessage) -> impl Future<Output = Result<(), SendError>> + Send;
}

/// Email verifikasi. Token di fragment URL (`#token=`) agar tidak masuk log server/Referer.
pub fn verification_email(to: &str, verify_url_base: &str, token: &str) -> EmailMessage {
    EmailMessage {
        to: to.to_owned(),
        template: "email_verification",
        subject: "Verifikasi email Anda".to_owned(),
        text_body: format!(
            "Klik tautan berikut untuk memverifikasi email Anda (berlaku 24 jam):\n{verify_url_base}#token={token}\n\nAbaikan email ini bila Anda tidak mendaftar."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_payload_has_no_secret_and_stable_key() {
        let u = Uuid::from_u128(7);
        let e = OutboxEvent::email_verification_requested(u);
        assert_eq!(e.payload, serde_json::json!({ "user_id": u }));
        assert_eq!(
            e.idempotency_key,
            format!("email_verification:{u}:registration")
        );
    }

    #[test]
    fn token_goes_in_fragment() {
        let m = verification_email("a@x.id", "https://app.ryasai.my.id/verify-email", "TOK");
        assert!(
            m.text_body
                .contains("https://app.ryasai.my.id/verify-email#token=TOK")
        );
        assert!(!m.text_body.contains("?token="));
    }
}
