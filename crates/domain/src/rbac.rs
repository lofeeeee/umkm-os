//! Otorisasi (P1-5, `PRD.md §3`, `PRD.md §6.2`): deny-by-default, permission sebagai
//! sumber otorisasi (role hanya preset), dan pemeriksaan object-level.

use std::collections::BTreeSet;

use umkm_shared::ids::TenantId;
use uuid::Uuid;

/// 33 permission key `PRD.md §3.3` (sama dengan seed migration 0002).
pub const ALL_PERMISSIONS: [&str; 33] = [
    "tenant.read",
    "tenant.update",
    "members.read",
    "members.invite",
    "members.manage",
    "website.read",
    "website.edit",
    "website.publish",
    "products.read",
    "products.create",
    "products.update",
    "products.archive",
    "inventory.read",
    "inventory.adjust",
    "orders.read",
    "orders.create",
    "orders.approve",
    "orders.reject",
    "orders.cancel",
    "tickets.read",
    "tickets.assign",
    "tickets.reply",
    "tickets.close",
    "conversations.read",
    "conversations.reply",
    "conversations.takeover",
    "ai_config.read",
    "ai_config.manage",
    "ai_provider.manage",
    "approvals.read",
    "approvals.decide",
    "analytics.read",
    "audit.read",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthzError {
    #[error("permission {0} diperlukan")]
    MissingPermission(&'static str),
    /// Objek milik tenant lain / tidak ditugaskan. Pemanggil HTTP memetakan ke 404
    /// agar keberadaan objek tidak bocor.
    #[error("objek tidak dapat diakses")]
    ObjectDenied,
}

/// Anggota tenant yang sudah diverifikasi (sesi + membership aktif + tenant aktif).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub user_id: Uuid,
    pub tenant_id: TenantId,
    pub role_name: String,
    permissions: BTreeSet<String>,
}

impl Actor {
    /// Permission di luar `ALL_PERMISSIONS` diabaikan (tidak memberi hak apa pun).
    pub fn new(
        user_id: Uuid,
        tenant_id: TenantId,
        role_name: String,
        permissions: impl IntoIterator<Item = String>,
    ) -> Self {
        let permissions = permissions
            .into_iter()
            .filter(|p| ALL_PERMISSIONS.contains(&p.as_str()))
            .collect();
        Self {
            user_id,
            tenant_id,
            role_name,
            permissions,
        }
    }

    pub fn has(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    /// Deny-by-default: hanya permission yang dimiliki yang lolos.
    pub fn require(&self, permission: &'static str) -> Result<(), AuthzError> {
        debug_assert!(
            ALL_PERMISSIONS.contains(&permission),
            "permission tak dikenal: {permission}"
        );
        if self.has(permission) {
            Ok(())
        } else {
            Err(AuthzError::MissingPermission(permission))
        }
    }

    /// Object-level: objek harus milik tenant actor (`PRD.md §6.2` ownership validation).
    pub fn require_same_tenant(&self, object_tenant: TenantId) -> Result<(), AuthzError> {
        if object_tenant == self.tenant_id {
            Ok(())
        } else {
            Err(AuthzError::ObjectDenied)
        }
    }

    /// Object-level untuk objek yang dapat ditugaskan (tiket/percakapan, P4/P6):
    /// pemegang `{scope}.assign` melihat semua; selain itu hanya yang ditugaskan padanya.
    pub fn require_assigned_or(
        &self,
        object_tenant: TenantId,
        assignee: Option<Uuid>,
        override_permission: &'static str,
    ) -> Result<(), AuthzError> {
        self.require_same_tenant(object_tenant)?;
        if self.has(override_permission) || assignee == Some(self.user_id) {
            Ok(())
        } else {
            Err(AuthzError::ObjectDenied)
        }
    }

    pub fn permissions(&self) -> impl Iterator<Item = &str> {
        self.permissions.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(perms: &[&str]) -> Actor {
        Actor::new(
            Uuid::from_u128(1),
            TenantId::from_uuid(Uuid::from_u128(10)),
            "X".into(),
            perms.iter().map(|s| s.to_string()),
        )
    }

    #[test]
    fn permission_list_has_33_unique_keys() {
        let set: BTreeSet<_> = ALL_PERMISSIONS.iter().collect();
        assert_eq!(set.len(), 33);
    }

    #[test]
    fn deny_by_default() {
        let a = actor(&[]);
        for p in ALL_PERMISSIONS {
            assert_eq!(a.require(p), Err(AuthzError::MissingPermission(p)));
        }
        let b = actor(&["orders.read"]);
        assert!(b.require("orders.read").is_ok());
        assert!(b.require("orders.approve").is_err());
    }

    #[test]
    fn unknown_permissions_grant_nothing() {
        let a = actor(&["*", "admin", "orders.*", "tenant.read "]);
        assert_eq!(a.permissions().count(), 0);
    }

    #[test]
    fn object_level_tenant_and_assignment() {
        let a = actor(&["tickets.read"]);
        let own = a.tenant_id;
        let other = TenantId::from_uuid(Uuid::from_u128(99));
        assert!(a.require_same_tenant(own).is_ok());
        assert_eq!(a.require_same_tenant(other), Err(AuthzError::ObjectDenied));
        // Ditugaskan ke saya -> boleh; ke orang lain -> tidak; tenant lain -> tidak
        assert!(
            a.require_assigned_or(own, Some(a.user_id), "tickets.assign")
                .is_ok()
        );
        assert_eq!(
            a.require_assigned_or(own, Some(Uuid::from_u128(2)), "tickets.assign"),
            Err(AuthzError::ObjectDenied)
        );
        assert_eq!(
            a.require_assigned_or(own, None, "tickets.assign"),
            Err(AuthzError::ObjectDenied)
        );
        assert_eq!(
            a.require_assigned_or(other, Some(a.user_id), "tickets.assign"),
            Err(AuthzError::ObjectDenied)
        );
        // Pemegang override melihat semua di tenant sendiri, tetap bukan tenant lain
        let m = actor(&["tickets.read", "tickets.assign"]);
        assert!(m.require_assigned_or(own, None, "tickets.assign").is_ok());
        assert_eq!(
            m.require_assigned_or(other, None, "tickets.assign"),
            Err(AuthzError::ObjectDenied)
        );
    }
}
