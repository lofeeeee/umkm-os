//! Audit event + redaksi (P1-8, `PRD.md §6.9`).
//!
//! "Jangan merekam password, API key, full payment credentials, atau message body
//! sensitif secara default." Redaksi dilakukan di domain sebelum data menyentuh
//! storage, rekursif ke seluruh objek/array.

use serde_json::{Map, Value};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

pub const REDACTED: &str = "[REDACTED]";

/// Aksi yang diaudit. Daftar akan bertambah per modul (P4-5: minimal 10 jenis event).
pub mod actions {
    pub const TENANT_REGISTERED: &str = "tenant.registered";
    pub const TENANT_UPDATED: &str = "tenant.updated";
    pub const AUTH_LOGIN_SUCCEEDED: &str = "auth.login.succeeded";
    pub const AUTH_LOGIN_FAILED: &str = "auth.login.failed";
    pub const AUTH_LOGOUT: &str = "auth.logout";
    pub const AUTH_EMAIL_VERIFIED: &str = "auth.email.verified";
    pub const TENANT_GUARD_PROBE_PROCESSED: &str = "tenant.guard_probe.processed";
    pub const INVENTORY_ADJUSTED: &str = "inventory.adjusted";
    pub const ORDER_CREATED: &str = "order.created";
    pub const PAYMENT_CLAIMED: &str = "payment.claimed";
}

/// Kategori minimum `PRD.md §6.9` → prefiks/aksi audit yang mewakilinya (P4-5).
/// Kategori dengan modul fase lanjut ditandai fasenya; dipakai uji cakupan & dokumentasi.
pub const PRD_CATEGORIES: [(&str, &[&str], Option<&str>); 10] = [
    (
        "Login/logout dan security events",
        &["auth.login.succeeded", "auth.login.failed", "auth.logout"],
        None,
    ),
    (
        "User invitation/role change",
        &["member.invited", "member.updated", "role.changed"],
        None,
    ),
    (
        "Business/domain/website changes",
        &[
            "business.updated",
            "website.publish",
            "commerce.settings_changed",
        ],
        None,
    ),
    (
        "Product dan stock changes",
        &["product.created", "product.updated", "inventory.adjusted"],
        None,
    ),
    (
        "Order create/approve/reject/cancel",
        &[
            "order.created",
            "order.approved",
            "order.rejected",
            "order.status_changed",
        ],
        None,
    ),
    (
        "Payment/refund events",
        &["payment.confirmed", "payment.rejected", "payment.refunded"],
        None,
    ),
    (
        "Ticket assignment/closure",
        &["ticket.assigned", "ticket.closed"],
        None,
    ),
    (
        "AI provider/config/key changes",
        &["ai.provider_created", "ai.key_rotated", "ai.config_updated"],
        None,
    ),
    (
        "Chat handover/approval",
        &["approval.approved", "approval.rejected"],
        Some("P6 (handover)"),
    ),
    ("Platform admin access", &[], Some("P7")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorType {
    User,
    Anonymous,
    System,
    Platform,
}

impl ActorType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Anonymous => "anonymous",
            Self::System => "system",
            Self::Platform => "platform",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub tenant_id: Option<TenantId>,
    pub actor_type: ActorType,
    pub actor_user_id: Option<Uuid>,
    pub action: &'static str,
    pub resource_type: &'static str,
    pub resource_id: Option<String>,
    pub request_id: Option<String>,
    pub ip: Option<String>,
    /// Sudah diredaksi (dijamin oleh konstruktor `with_summary`).
    summary: Value,
}

impl AuditEvent {
    pub fn new(action: &'static str, resource_type: &'static str) -> Self {
        Self {
            tenant_id: None,
            actor_type: ActorType::Anonymous,
            actor_user_id: None,
            action,
            resource_type,
            resource_id: None,
            request_id: None,
            ip: None,
            summary: Value::Object(Map::new()),
        }
    }

    pub fn by_user(mut self, user_id: Uuid) -> Self {
        self.actor_type = ActorType::User;
        self.actor_user_id = Some(user_id);
        self
    }

    pub fn in_tenant(mut self, tenant: TenantId) -> Self {
        self.tenant_id = Some(tenant);
        self
    }

    pub fn resource(mut self, id: impl ToString) -> Self {
        self.resource_id = Some(id.to_string());
        self
    }

    pub fn request(mut self, request_id: Option<&str>, ip: Option<&str>) -> Self {
        self.request_id = request_id.map(str::to_owned);
        self.ip = ip.map(|s| s.chars().take(64).collect());
        self
    }

    /// Ringkasan before/after; selalu diredaksi.
    pub fn with_summary(mut self, summary: Value) -> Self {
        self.summary = redact(summary);
        self
    }

    pub fn summary(&self) -> &Value {
        &self.summary
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase().replace(['-', ' '], "_");
    const EXACT: [&str; 6] = ["key", "pin", "otp", "cvv", "cvc", "pan"];
    const CONTAINS: [&str; 15] = [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "authorization",
        "cookie",
        "credential",
        "private_key",
        "card_number",
        "cardnumber",
        "message_body",
        "body",
        "content",
    ];
    EXACT.contains(&k.as_str()) || CONTAINS.iter().any(|c| k.contains(c)) || k.ends_with("_key")
}

/// Ganti nilai field sensitif (rekursif) dengan `[REDACTED]`.
pub fn redact(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let v = if is_sensitive_key(&k) {
                        Value::String(REDACTED.into())
                    } else {
                        redact(v)
                    };
                    (k, v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(redact).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn redacts_sensitive_keys_recursively() {
        let input = json!({
            "name": "Toko",
            "password": "rahasia",
            "new_password_hash": "$argon2id$...",
            "api_key": "sk-123",
            "Authorization": "Bearer x",
            "provider": {"base_url": "https://x", "secret_key": "s", "headers": [{"cookie": "c"}, {"x": 1}]},
            "session_token": "t",
            "payment": {"card_number": "4111", "cvv": "123", "last4": "1111"},
            "message": {"body": "isi pesan pelanggan", "id": 5},
            "otp": "000000"
        });
        let out = redact(input);
        let s = out.to_string();
        for leaked in [
            "rahasia",
            "$argon2id",
            "sk-123",
            "Bearer",
            "\"s\"",
            "\"c\"",
            "\"t\"",
            "4111\"",
            "123\"",
            "isi pesan",
            "000000",
        ] {
            assert!(!s.contains(leaked), "bocor {leaked}: {s}");
        }
        assert_eq!(out["name"], "Toko");
        assert_eq!(out["provider"]["base_url"], "https://x");
        assert_eq!(out["provider"]["headers"][1]["x"], 1);
        assert_eq!(out["payment"]["last4"], "1111");
        assert_eq!(out["message"]["id"], 5);
    }

    #[test]
    fn builder_always_redacts_summary() {
        let e = AuditEvent::new(actions::TENANT_UPDATED, "tenant")
            .with_summary(json!({"before": {"name": "A"}, "after": {"name": "B", "token": "x"}}));
        assert_eq!(e.summary()["after"]["token"], REDACTED);
        assert_eq!(e.summary()["before"]["name"], "A");
    }

    #[test]
    fn action_names_match_db_check() {
        for a in [
            actions::TENANT_REGISTERED,
            actions::TENANT_UPDATED,
            actions::AUTH_LOGIN_SUCCEEDED,
            actions::AUTH_LOGIN_FAILED,
            actions::AUTH_LOGOUT,
            actions::AUTH_EMAIL_VERIFIED,
        ] {
            let parts: Vec<&str> = a.split('.').collect();
            assert!(
                parts.len() >= 2
                    && parts
                        .iter()
                        .all(|p| !p.is_empty()
                            && p.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')),
                "{a}"
            );
        }
    }
}
