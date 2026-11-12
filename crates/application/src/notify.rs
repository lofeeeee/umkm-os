//! Event outbox dan port pengiriman email (P1-6, `PRD.md §4.13`).
//!
//! Provider email belum diputuskan (D-08 ditunda): port ini provider-neutral.

use std::future::Future;

use uuid::Uuid;

/// Tipe event yang dikenal worker. Payload tidak pernah berisi secret.
pub mod events {
    pub const EMAIL_VERIFICATION_REQUESTED: &str = "auth.email_verification_requested";
    /// Canary isolasi (P1-10, `PRD.md §13.2` #5): diproses di tenant context event.
    pub const TENANT_GUARD_PROBE: &str = "tenant.guard_probe";
    /// Stok tersedia varian turun melewati `min_stock` (P3-2, AC-INV-002).
    pub const INVENTORY_LOW_STOCK: &str = "inventory.low_stock";
    /// Order baru masuk (P3-5): notifikasi staf menyusul NTF/D-08 (log-only).
    pub const ORDER_CREATED: &str = "order.created";
    /// Verifikasi email akun customer toko (P3-3, ADR-0005). Token dibuat worker.
    pub const CUSTOMER_EMAIL_VERIFICATION_REQUESTED: &str = "customer.email_verification_requested";
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

impl OutboxEvent {
    pub fn customer_email_verification_requested(tenant_id: Uuid, account_id: Uuid) -> Self {
        Self {
            tenant_id: Some(tenant_id),
            event_type: events::CUSTOMER_EMAIL_VERIFICATION_REQUESTED,
            payload: serde_json::json!({ "account_id": account_id }),
            idempotency_key: format!("customer_email_verification:{account_id}:registration"),
        }
    }

    /// Satu event per persilangan ambang; `cause_id` = movement/reservasi penyebabnya.
    pub fn inventory_low_stock(
        tenant_id: Uuid,
        variant_id: Uuid,
        available: i64,
        min_stock: i64,
        cause_id: Uuid,
    ) -> Self {
        Self {
            tenant_id: Some(tenant_id),
            event_type: events::INVENTORY_LOW_STOCK,
            payload: serde_json::json!({
                "variant_id": variant_id, "available": available, "min_stock": min_stock
            }),
            idempotency_key: format!("inventory_low_stock:{variant_id}:{cause_id}"),
        }
    }
}

impl OutboxEvent {
    /// `resource_tenant_id` = tenant pemilik resource yang akan dibaca job. Untuk probe
    /// normal sama dengan `tenant_id`; berbeda = upaya lintas tenant yang wajib gagal.
    pub fn tenant_guard_probe(tenant_id: Uuid, resource_tenant_id: Uuid, nonce: Uuid) -> Self {
        Self {
            tenant_id: Some(tenant_id),
            event_type: events::TENANT_GUARD_PROBE,
            payload: serde_json::json!({ "resource_tenant_id": resource_tenant_id }),
            idempotency_key: format!("tenant_guard_probe:{tenant_id}:{nonce}"),
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

/// Email verifikasi akun customer: tautan ke situs toko itu sendiri (host-only).
pub fn customer_verification_email(to: &str, shop_host: &str, token: &str) -> EmailMessage {
    EmailMessage {
        to: to.to_owned(),
        template: "customer_email_verification",
        subject: "Verifikasi email akun toko Anda".to_owned(),
        text_body: format!(
            "Klik tautan berikut untuk memverifikasi email Anda (berlaku 24 jam):\nhttps://{shop_host}/akun/verifikasi#token={token}\n\nAbaikan email ini bila Anda tidak mendaftar."
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
