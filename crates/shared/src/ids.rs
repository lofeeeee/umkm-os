//! Identifier bertipe. UUID PK sesuai `PRD.md §5.2`.

use std::fmt;

/// ID tenant. Harus berasal dari hostname/session/membership yang terverifikasi
/// (`PRD.md §5.7`), tidak pernah dari body/query klien sebagai bukti.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TenantId(uuid::Uuid);

impl TenantId {
    pub const fn from_uuid(id: uuid::Uuid) -> Self {
        Self(id)
    }

    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    pub const fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
