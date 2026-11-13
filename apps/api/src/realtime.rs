//! Realtime minimum (P1-10, `PRD.md §7.5`, `PRD.md §13.2` #7): SSE per tenant dengan
//! TenantGuard.
//!
//! - Hub in-process (satu instance API). Fan-out multi-instance via Redis: P4-6.
//! - Otorisasi saat koneksi (sesi + membership + `tenant.read`) DAN pada setiap event
//!   yang dikirim (membership dicek ulang; dicabut -> stream berakhir).
//! - Setiap event membawa `tenant_id`; stream hanya meneruskan event tenant miliknya.
//! - Jumlah koneksi dibatasi (`PRD.md §7.5`).

use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, stream};
use serde::Serialize;
use tokio::sync::broadcast;
use umkm_infrastructure::{
    rbac_repo,
    realtime::RtMessage,
    redis::{KeyNamespace, RedisClient},
};
use umkm_shared::{http::ErrorCode, ids::TenantId};

use crate::{ApiError, AppState, session::Member};

#[derive(Debug, Clone, Serialize)]
pub struct TenantEvent {
    #[serde(skip)]
    pub tenant_id: TenantId,
    pub kind: String,
    pub data: serde_json::Value,
    /// Hanya untuk pengguna ini (notifikasi pribadi); `None` = semua anggota yang berhak.
    #[serde(skip)]
    pub user_id: Option<uuid::Uuid>,
    /// Hanya untuk anggota yang memiliki permission ini (dicek ulang per event).
    #[serde(skip)]
    pub permission: Option<String>,
}

impl TenantEvent {
    pub fn tenant(tenant_id: TenantId, kind: &str, data: serde_json::Value) -> Self {
        Self {
            tenant_id,
            kind: kind.to_owned(),
            data,
            user_id: None,
            permission: None,
        }
    }
}

impl From<RtMessage> for TenantEvent {
    fn from(m: RtMessage) -> Self {
        Self {
            tenant_id: TenantId::from_uuid(m.tenant_id),
            kind: m.kind,
            data: m.data,
            user_id: m.user_id,
            permission: m.permission,
        }
    }
}

/// Kirim event: lewat Redis bila tersedia (semua instance, termasuk diri sendiri, menerimanya
/// lewat bridge), selain itu langsung ke hub lokal.
pub async fn emit(state: &AppState, ev: TenantEvent) {
    match &state.rt_publisher {
        Some(p) => {
            let m = RtMessage {
                tenant_id: *ev.tenant_id.as_uuid(),
                kind: ev.kind,
                data: ev.data,
                user_id: ev.user_id,
                permission: ev.permission,
            };
            if let Err(e) = p.publish(&m).await {
                tracing::warn!(error = %e, "realtime publish failed");
            }
        }
        None => state.realtime.publish(ev),
    }
}

/// Bridge Redis → hub lokal; menyambung ulang bila koneksi putus.
pub fn spawn_bridge(
    hub: RealtimeHub,
    client: Arc<RedisClient>,
    ns: KeyNamespace,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match umkm_infrastructure::realtime::subscribe(&client, &ns).await {
                Ok(s) => {
                    futures_util::pin_mut!(s);
                    while let Some(m) = futures_util::StreamExt::next(&mut s).await {
                        hub.publish(m.into());
                    }
                    tracing::warn!("realtime bridge disconnected; reconnecting");
                }
                Err(e) => tracing::warn!(error = %e, "realtime bridge subscribe failed"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

#[derive(Clone)]
pub struct RealtimeHub {
    tx: broadcast::Sender<TenantEvent>,
    open: Arc<AtomicUsize>,
    max_connections: usize,
}

impl Default for RealtimeHub {
    fn default() -> Self {
        Self::new(1024, 1000)
    }
}

impl RealtimeHub {
    pub fn new(buffer: usize, max_connections: usize) -> Self {
        let (tx, _) = broadcast::channel(buffer);
        Self {
            tx,
            open: Arc::new(AtomicUsize::new(0)),
            max_connections,
        }
    }

    /// Publikasikan event milik satu tenant. Tanpa subscriber = no-op.
    pub fn publish(&self, event: TenantEvent) {
        let _ = self.tx.send(event);
    }

    pub fn open_connections(&self) -> usize {
        self.open.load(Ordering::Relaxed)
    }

    /// Ambil satu slot koneksi (batas `PRD.md §7.5`) + penerima event; `None` = penuh.
    pub(crate) fn connect(&self) -> Option<(broadcast::Receiver<TenantEvent>, ConnGuard)> {
        let prev = self.open.fetch_add(1, Ordering::Relaxed);
        let guard = ConnGuard(self.open.clone());
        if prev >= self.max_connections {
            return None;
        }
        Some((self.tx.subscribe(), guard))
    }
}

/// Menurunkan hitungan koneksi saat stream di-drop (klien putus).
pub(crate) struct ConnGuard(Arc<AtomicUsize>);

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Stream event tenant (SSE). Butuh `tenant.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/events", tag = "tenants",
    params(("tenant_id" = uuid::Uuid, Path, description = "Selector tenant; diverifikasi terhadap membership")),
    responses(
        (status = 200, description = "text/event-stream; hanya event tenant ini", content_type = "text/event-stream", body = String),
        (status = 401, description = "Tidak ada sesi valid", body = umkm_shared::http::ErrorEnvelope),
        (status = 403, description = "Permission kurang", body = umkm_shared::http::ErrorEnvelope),
        (status = 404, description = "Bukan anggota", body = umkm_shared::http::ErrorEnvelope),
        (status = 503, description = "Batas koneksi realtime tercapai", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn tenant_events(
    State(state): State<AppState>,
    Member(actor): Member,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    actor.require("tenant.read")?;
    let hub = state.realtime.clone();
    let prev = hub.open.fetch_add(1, Ordering::Relaxed);
    let guard = ConnGuard(hub.open.clone());
    if prev >= hub.max_connections {
        drop(guard);
        return Err(ApiError::new(ErrorCode::ServiceUnavailable));
    }
    let pool = state
        .auth
        .as_ref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))?
        .pool
        .clone();
    let rx = hub.tx.subscribe();
    let (user, tenant) = (actor.user_id, actor.tenant_id);

    let s = stream::unfold((rx, guard, pool), move |(mut rx, guard, pool)| async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    // TenantGuard 1: event tenant lain tidak pernah diteruskan.
                    if ev.tenant_id != tenant {
                        continue;
                    }
                    // TenantGuard 2: event pribadi hanya untuk penerimanya.
                    if ev.user_id.is_some_and(|u| u != user) {
                        continue;
                    }
                    // TenantGuard 3: otorisasi dicek ulang per event (PRD.md §7.5).
                    let actor = match rbac_repo::load_actor(&pool, user, tenant).await {
                        Ok(Some(a)) if a.has("tenant.read") => a,
                        _ => return None, // membership dicabut/DB error -> tutup stream
                    };
                    // TenantGuard 4: event ber-permission hanya untuk yang berhak saat ini.
                    if ev.permission.as_deref().is_some_and(|p| !actor.has(p)) {
                        continue;
                    }
                    // Event chat (P6-2/3): staf hanya menerima ping; isi diambil ulang lewat API
                    // viewer yang memasking sesuai permission (FR-CNV-003) — SSE tidak boleh jadi jalan pintas.
                    let data = if ev.kind.starts_with("chat.") {
                        serde_json::json!({ "conversation_id": ev.data.get("conversation_id") })
                    } else {
                        ev.data.clone()
                    };
                    let event = Event::default()
                        .event(ev.kind.clone())
                        .json_data(&data)
                        .ok()?;
                    return Some((Ok(event), (rx, guard, pool)));
                }
                // Tertinggal: lewati event yang hilang (klien dapat refetch state).
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Ok(Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
