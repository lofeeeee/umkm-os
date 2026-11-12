//! Jembatan realtime lintas instance (P4-6, `PRD.md §7.5`): Redis pub/sub satu kanal platform.
//! Publisher: worker (notifikasi) & instance API. Subscriber: setiap instance API meneruskan ke
//! hub lokalnya; TenantGuard tetap di API (tenant + target user + permission + membership per event).
//!
//! Pesan dari kanal diperlakukan tidak tepercaya: divalidasi bentuk & ukurannya, yang rusak dibuang.

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use umkm_domain::rbac::ALL_PERMISSIONS;
use uuid::Uuid;

use crate::redis::{KeyNamespace, RedisClient, RedisError};

pub const MAX_PAYLOAD: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RtMessage {
    pub tenant_id: Uuid,
    /// `[a-z_]+(\.[a-z_]+)*`, ≤ 60
    pub kind: String,
    pub data: serde_json::Value,
    /// Hanya untuk pengguna ini (mis. notifikasi pribadi)
    pub user_id: Option<Uuid>,
    /// Hanya untuk anggota yang memiliki permission ini
    pub permission: Option<String>,
}

impl RtMessage {
    pub fn is_valid(&self) -> bool {
        let k = &self.kind;
        !k.is_empty()
            && k.len() <= 60
            && !k.starts_with('.')
            && !k.ends_with('.')
            && k.bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_' || b == b'.')
            && self
                .permission
                .as_deref()
                .is_none_or(|p| ALL_PERMISSIONS.contains(&p))
            && self.data.is_object()
    }

    /// Parse + validasi pesan dari kanal.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_PAYLOAD {
            return None;
        }
        serde_json::from_slice::<Self>(bytes)
            .ok()
            .filter(Self::is_valid)
    }
}

/// Nama kanal ber-namespace (lingkungan berbeda tidak saling mendengar).
pub fn channel(ns: &KeyNamespace) -> String {
    ns.platform()
        .key("rt", "events")
        .map(|k| k.as_str().to_owned())
        .unwrap_or_else(|_| "umkm:platform:rt:events".to_owned())
}

#[derive(Clone)]
pub struct RtPublisher {
    conn: ::redis::aio::ConnectionManager,
    channel: String,
}

impl RtPublisher {
    pub fn new(client: &RedisClient, ns: &KeyNamespace) -> Result<Self, RedisError> {
        Ok(Self {
            conn: client.connection_manager_lazy()?,
            channel: channel(ns),
        })
    }

    /// Best-effort: kegagalan dikembalikan agar pemanggil mencatat, tidak menggagalkan transaksi bisnis.
    pub async fn publish(&self, m: &RtMessage) -> Result<(), RedisError> {
        if !m.is_valid() {
            return Ok(());
        }
        let body = serde_json::to_vec(m).unwrap_or_default();
        if body.len() > MAX_PAYLOAD {
            return Ok(());
        }
        let mut c = self.conn.clone();
        let _: i64 = ::redis::cmd("PUBLISH")
            .arg(&self.channel)
            .arg(body)
            .query_async(&mut c)
            .await
            .map_err(RedisError::from)?;
        Ok(())
    }
}

/// Langganan kanal; menghasilkan pesan valid saja. Stream berakhir bila koneksi putus
/// (pemanggil menyambung ulang).
pub async fn subscribe(
    client: &RedisClient,
    ns: &KeyNamespace,
) -> Result<impl Stream<Item = RtMessage> + Send + 'static, RedisError> {
    let mut ps = client
        .raw()
        .get_async_pubsub()
        .await
        .map_err(RedisError::from)?;
    ps.subscribe(channel(ns)).await.map_err(RedisError::from)?;
    Ok(ps
        .into_on_message()
        .filter_map(|m| async move { RtMessage::parse(m.get_payload_bytes()) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(kind: &str, perm: Option<&str>) -> RtMessage {
        RtMessage {
            tenant_id: Uuid::nil(),
            kind: kind.into(),
            data: serde_json::json!({}),
            user_id: None,
            permission: perm.map(Into::into),
        }
    }

    #[test]
    fn validation() {
        assert!(msg("notification.created", Some("orders.read")).is_valid());
        assert!(!msg("Bad Kind", None).is_valid());
        assert!(!msg("x", Some("root.all")).is_valid());
        assert!(!msg(".x", None).is_valid());
        assert!(RtMessage::parse(b"{\"tenant_id\":\"00000000-0000-0000-0000-000000000000\",\"kind\":\"a\",\"data\":{},\"user_id\":null,\"permission\":null}").is_some());
        assert!(RtMessage::parse(b"{\"tenant_id\":\"x\"}").is_none());
        assert!(RtMessage::parse(&vec![b' '; MAX_PAYLOAD + 1]).is_none());
        let mut m = msg("a", None);
        m.data = serde_json::json!([1]);
        assert!(!m.is_valid());
    }
}
