//! Notifikasi (P4-4, `PRD.md §4.13`). Worker: event outbox → notifikasi in-app per penerima
//! (+ email customer untuk status order). Penerima dihitung di tenant context event (RLS),
//! sehingga event tenant A tidak pernah menjangkau anggota tenant B (AC-NTF-001).
//!
//! Idempotensi (AC-NTF-002/003): notifikasi unik per (tenant, user, event sumber) — worker yang
//! mengulang setelah crash tidak membuat duplikat. Email: baris `notification_deliveries` ditulis
//! sebelum kirim; kunci (event, penerima) = idempotency key provider; status `sent` → tidak dikirim ulang.

use std::collections::BTreeSet;

use sqlx::{PgPool, Row};
use umkm_application::notify::{EmailMessage, EmailSender, events};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::{Tx, tenant_tx};

/// Event AI handover (P6) & status langganan (P7): aturan penerima sudah ada di sini.
pub const AI_HANDOVER: &str = "ai.handover";
pub const SUBSCRIPTION_STATUS: &str = "subscription.status";

/// 9 event notifikasi PRD §4.13 → tipe outbox yang memicunya.
pub const NOTIFIABLE: [&str; 11] = [
    events::ORDER_CREATED,
    events::ORDER_STATUS_CHANGED,
    events::PAYMENT_CLAIMED,
    crate::ticket_repo::TICKET_CREATED,
    crate::ticket_repo::TICKET_ASSIGNED,
    crate::ticket_repo::TICKET_CUSTOMER_REPLY,
    crate::approval_repo::APPROVAL_REQUESTED,
    crate::approval_repo::APPROVAL_DECIDED,
    events::INVENTORY_LOW_STOCK,
    AI_HANDOVER,
    SUBSCRIPTION_STATUS,
];

#[derive(Debug, thiserror::Error)]
pub enum DeliverError {
    #[error("payload tidak valid")]
    InvalidPayload,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("pengiriman email gagal ({0})")]
    Send(&'static str, bool),
}

impl DeliverError {
    pub fn class(&self) -> (&'static str, bool) {
        match self {
            Self::InvalidPayload => ("invalid_payload", false),
            Self::Db(_) => ("db_error", true),
            Self::Send(c, r) => (c, *r),
        }
    }
}

struct Note {
    title: String,
    body: String,
    link: Option<String>,
}

fn uuid_at(p: &serde_json::Value, path: &[&str]) -> Option<Uuid> {
    let mut v = p;
    for k in path {
        v = v.get(k)?;
    }
    v.as_str()?.parse().ok()
}

async fn with_permission(tx: &mut Tx, t: TenantId, perm: &str) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT m.user_id FROM tenant_memberships m
         JOIN role_permissions rp ON rp.role_id = m.role_id
         JOIN permissions p ON p.id = rp.permission_id
         WHERE m.tenant_id = $1 AND m.status = 'active' AND p.permission_key = $2",
    )
    .bind(t.as_uuid())
    .bind(perm)
    .fetch_all(&mut **tx)
    .await
}

async fn active_member(tx: &mut Tx, t: TenantId, user: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2 AND status = 'active')")
        .bind(t.as_uuid())
        .bind(user)
        .fetch_one(&mut **tx)
        .await
}

async fn order_code(
    tx: &mut Tx,
    t: TenantId,
    id: Uuid,
) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
    let r = sqlx::query("SELECT code, customer_email FROM orders WHERE tenant_id = $1 AND id = $2")
        .bind(t.as_uuid())
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(r.map(|r| (r.get("code"), r.get("customer_email"))))
}

async fn ticket_info(
    tx: &mut Tx,
    t: TenantId,
    id: Uuid,
) -> Result<Option<(String, String, Option<Uuid>)>, sqlx::Error> {
    let r = sqlx::query(
        "SELECT code, subject, assignee_user_id FROM tickets WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t.as_uuid())
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(r.map(|r| (r.get("code"), r.get("subject"), r.get("assignee_user_id"))))
}

const STATUS_ID: [(&str, &str); 9] = [
    ("PAID", "dibayar"),
    ("APPROVED", "disetujui"),
    ("REJECTED", "ditolak"),
    ("READY_FOR_PICKUP", "siap diambil"),
    ("SHIPPED", "dikirim"),
    ("COMPLETED", "selesai"),
    ("CANCELLED", "dibatalkan"),
    ("EXPIRED", "kedaluwarsa"),
    ("REFUNDED", "dananya dikembalikan"),
];

/// Hitung penerima + isi untuk satu event. `None` = event ini tidak menghasilkan notifikasi in-app.
async fn plan(
    tx: &mut Tx,
    t: TenantId,
    kind: &str,
    p: &serde_json::Value,
) -> Result<(BTreeSet<Uuid>, Option<Note>, Option<EmailMessage>), DeliverError> {
    let q = format!("?tenant={}", t.as_uuid());
    let mut to = BTreeSet::new();
    let mut email = None;
    let note = match kind {
        k if k == events::ORDER_CREATED => {
            let id = uuid_at(p, &["order_id"]).ok_or(DeliverError::InvalidPayload)?;
            let Some((code, _)) = order_code(tx, t, id).await? else {
                return Ok((to, None, None));
            };
            to.extend(with_permission(tx, t, "orders.approve").await?);
            Some(Note {
                title: "Pesanan baru".into(),
                body: format!("Pesanan {code} masuk dan menunggu pembayaran."),
                link: Some(format!("/orders{q}")),
            })
        }
        k if k == events::ORDER_STATUS_CHANGED => {
            let id = uuid_at(p, &["order_id"]).ok_or(DeliverError::InvalidPayload)?;
            let status = p
                .get("status")
                .and_then(|s| s.as_str())
                .ok_or(DeliverError::InvalidPayload)?;
            let Some((code, customer_email)) = order_code(tx, t, id).await? else {
                return Ok((to, None, None));
            };
            // Customer menerima kabar perubahan status (PRD §4.7); email log-only sampai D-08.
            if let (Some(e), Some((_, word))) =
                (customer_email, STATUS_ID.iter().find(|(s, _)| *s == status))
            {
                email = Some(EmailMessage {
                    to: e,
                    template: "order_status",
                    subject: format!("Pesanan {code} {word}"),
                    text_body: format!("Status pesanan {code} kini: {word}."),
                });
            }
            if matches!(status, "APPROVED" | "REJECTED") {
                to.extend(with_permission(tx, t, "orders.approve").await?);
                let word = if status == "APPROVED" {
                    "disetujui"
                } else {
                    "ditolak"
                };
                Some(Note {
                    title: format!("Pesanan {word}"),
                    body: format!("Pesanan {code} {word}."),
                    link: Some(format!("/orders{q}")),
                })
            } else {
                None
            }
        }
        k if k == events::PAYMENT_CLAIMED => {
            let id = uuid_at(p, &["order_id"]).ok_or(DeliverError::InvalidPayload)?;
            let Some((code, _)) = order_code(tx, t, id).await? else {
                return Ok((to, None, None));
            };
            to.extend(with_permission(tx, t, "payments.verify").await?);
            Some(Note {
                title: "Pembayaran perlu dicek".into(),
                body: format!("Pembeli menandai pesanan {code} sudah dibayar."),
                link: Some(format!("/orders{q}")),
            })
        }
        k if k == crate::ticket_repo::TICKET_CREATED
            || k == crate::ticket_repo::TICKET_CUSTOMER_REPLY
            || k == crate::ticket_repo::TICKET_ASSIGNED =>
        {
            let id = uuid_at(p, &["ticket_id"]).ok_or(DeliverError::InvalidPayload)?;
            let Some((code, subject, assignee)) = ticket_info(tx, t, id).await? else {
                return Ok((to, None, None));
            };
            let (title, body) = if k == crate::ticket_repo::TICKET_ASSIGNED {
                let u = uuid_at(p, &["detail", "assignee"]).ok_or(DeliverError::InvalidPayload)?;
                to.insert(u);
                (
                    "Ticket ditugaskan kepada Anda",
                    format!("{code}: {subject}"),
                )
            } else if k == crate::ticket_repo::TICKET_CUSTOMER_REPLY {
                match assignee {
                    Some(u) => {
                        to.insert(u);
                    }
                    None => to.extend(with_permission(tx, t, "tickets.assign").await?),
                }
                ("Balasan pelanggan", format!("{code}: {subject}"))
            } else {
                to.extend(with_permission(tx, t, "tickets.assign").await?);
                ("Ticket baru", format!("{code}: {subject}"))
            };
            Some(Note {
                title: title.into(),
                body,
                link: Some(format!("/tickets{q}")),
            })
        }
        k if k == crate::approval_repo::APPROVAL_REQUESTED
            || k == crate::approval_repo::APPROVAL_DECIDED =>
        {
            let id = uuid_at(p, &["request_id"]).ok_or(DeliverError::InvalidPayload)?;
            let Some(r) = sqlx::query("SELECT summary, requested_by, steps, current_step, status FROM approval_requests WHERE tenant_id = $1 AND id = $2")
                .bind(t.as_uuid())
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?
            else {
                return Ok((to, None, None));
            };
            let summary: String = r.get("summary");
            let requester: Uuid = r.get("requested_by");
            if k == crate::approval_repo::APPROVAL_REQUESTED {
                let steps: sqlx::types::Json<Vec<umkm_domain::approvals::Step>> = r.get("steps");
                let deciders = with_permission(tx, t, "approvals.decide").await?;
                let step = steps
                    .0
                    .first()
                    .cloned()
                    .unwrap_or(umkm_domain::approvals::Step {
                        role_ids: vec![],
                        user_ids: vec![],
                    });
                for u in deciders {
                    let role: Option<Uuid> = sqlx::query_scalar("SELECT role_id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2")
                        .bind(t.as_uuid())
                        .bind(u)
                        .fetch_optional(&mut **tx)
                        .await?;
                    let eligible = (step.role_ids.is_empty() && step.user_ids.is_empty())
                        || step.user_ids.contains(&u)
                        || role.is_some_and(|r| step.role_ids.contains(&r));
                    if eligible && u != requester {
                        to.insert(u);
                    }
                }
                Some(Note {
                    title: "Permintaan persetujuan".into(),
                    body: summary,
                    link: Some(format!("/approvals{q}")),
                })
            } else {
                let status: String = r.get("status");
                if status == "pending" {
                    None
                } else {
                    to.insert(requester);
                    Some(Note {
                        title: "Permintaan Anda diputus".into(),
                        body: format!("{summary} — {status}"),
                        link: Some(format!("/approvals{q}")),
                    })
                }
            }
        }
        k if k == events::INVENTORY_LOW_STOCK => {
            let v = uuid_at(p, &["variant_id"]).ok_or(DeliverError::InvalidPayload)?;
            let name: Option<String> = sqlx::query_scalar(
                "SELECT p.name || ' (' || v.sku || ')' FROM product_variants v JOIN products p ON p.tenant_id = v.tenant_id AND p.id = v.product_id
                 WHERE v.tenant_id = $1 AND v.id = $2",
            )
            .bind(t.as_uuid())
            .bind(v)
            .fetch_optional(&mut **tx)
            .await?;
            let Some(name) = name else {
                return Ok((to, None, None));
            };
            to.extend(with_permission(tx, t, "inventory.adjust").await?);
            let avail = p
                .get("available")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            Some(Note {
                title: "Stok menipis".into(),
                body: format!("{name} tersisa {avail}."),
                link: Some(format!("/inventory{q}")),
            })
        }
        AI_HANDOVER => {
            to.extend(with_permission(tx, t, "conversations.takeover").await?);
            Some(Note {
                title: "Percakapan perlu diambil alih".into(),
                body: "AI meminta bantuan agen untuk percakapan pelanggan.".into(),
                link: Some(format!("/conversations{q}")),
            })
        }
        SUBSCRIPTION_STATUS => {
            to.extend(with_permission(tx, t, "tenant.update").await?);
            let s = p
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("berubah");
            Some(Note {
                title: "Status langganan".into(),
                body: format!("Status langganan toko: {s}."),
                link: None,
            })
        }
        _ => None,
    };
    // Penerima hanya anggota aktif tenant event (pertahanan kedua di samping RLS).
    let mut active = BTreeSet::new();
    for u in to {
        if active_member(tx, t, u).await? {
            active.insert(u);
        }
    }
    Ok((active, note, email))
}

/// Proses satu event outbox di tenant context-nya.
pub async fn deliver(
    pool: &PgPool,
    sender: &impl EmailSender,
    event_id: Uuid,
    tenant: Option<Uuid>,
    kind: &str,
    payload: &serde_json::Value,
) -> Result<Delivered, DeliverError> {
    let t = TenantId::from_uuid(tenant.ok_or(DeliverError::InvalidPayload)?);
    let mut tx = tenant_tx(pool, t).await?;
    let (to, note, email) = plan(&mut tx, t, kind, payload).await?;
    let mut created = 0;
    if let Some(n) = &note {
        for u in &to {
            created += sqlx::query(
                "INSERT INTO notifications (tenant_id, user_id, event_type, title, body, link, source_event_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (tenant_id, user_id, source_event_id) DO NOTHING",
            )
            .bind(t.as_uuid())
            .bind(u)
            .bind(kind)
            .bind(&n.title)
            .bind(n.body.chars().take(500).collect::<String>())
            .bind(&n.link)
            .bind(event_id)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;
        }
    }
    // Catat niat kirim email di transaksi yang sama.
    let pending_email = match &email {
        Some(m) => {
            let h = crate::auth::subject_digest(&m.to.to_lowercase());
            sqlx::query(
                "INSERT INTO notification_deliveries (tenant_id, source_event_id, channel, recipient_hash) VALUES ($1, $2, 'email', $3)
                 ON CONFLICT (tenant_id, source_event_id, channel, recipient_hash) DO NOTHING",
            )
            .bind(t.as_uuid())
            .bind(event_id)
            .bind(&h)
            .execute(&mut *tx)
            .await?;
            let status: String = sqlx::query_scalar(
                "SELECT status FROM notification_deliveries WHERE tenant_id = $1 AND source_event_id = $2 AND channel = 'email' AND recipient_hash = $3",
            )
            .bind(t.as_uuid())
            .bind(event_id)
            .bind(&h)
            .fetch_one(&mut *tx)
            .await?;
            (status == "pending").then_some(h)
        }
        None => None,
    };
    tx.commit().await?;
    if let (Some(m), Some(h)) = (email, pending_email) {
        sender
            .send(&m)
            .await
            .map_err(|e| DeliverError::Send(e.class, e.retryable))?;
        let mut tx = tenant_tx(pool, t).await?;
        sqlx::query(
            "UPDATE notification_deliveries SET status = 'sent', sent_at = now()
             WHERE tenant_id = $1 AND source_event_id = $2 AND channel = 'email' AND recipient_hash = $3",
        )
        .bind(t.as_uuid())
        .bind(event_id)
        .bind(&h)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    let _ = created;
    Ok(Delivered {
        tenant: t,
        recipients: if note.is_some() {
            to.into_iter().collect()
        } else {
            vec![]
        },
        title: note.as_ref().map(|n| n.title.clone()),
        link: note.and_then(|n| n.link),
    })
}

/// Hasil pengiriman untuk fan-out realtime (P4-6).
#[derive(Debug, Clone)]
pub struct Delivered {
    pub tenant: TenantId,
    pub recipients: Vec<Uuid>,
    pub title: Option<String>,
    pub link: Option<String>,
}

/// Permission baca yang membatasi siaran tenant per jenis event.
pub fn broadcast_permission(kind: &str) -> &'static str {
    match kind {
        k if k.starts_with("order.") || k.starts_with("payment.") => "orders.read",
        k if k.starts_with("ticket.") => "tickets.read",
        k if k.starts_with("approval.") => "approvals.read",
        k if k.starts_with("inventory.") => "inventory.read",
        AI_HANDOVER => "conversations.read",
        _ => "tenant.update",
    }
}

// ------------------------------------------------------------------ baca (pengguna)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: Uuid,
    pub event_type: String,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    pub read: bool,
    pub created_at: String,
}

pub async fn list(
    pool: &PgPool,
    t: TenantId,
    user: Uuid,
    unread_only: bool,
    limit: i64,
) -> Result<(Vec<Notification>, i64), sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let unread: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE tenant_id = $1 AND user_id = $2 AND read_at IS NULL")
        .bind(t.as_uuid())
        .bind(user)
        .fetch_one(&mut *tx)
        .await?;
    let rows = sqlx::query(
        "SELECT id, event_type, title, body, link, read_at IS NOT NULL AS read,
                to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at
         FROM notifications WHERE tenant_id = $1 AND user_id = $2 AND (NOT $3 OR read_at IS NULL)
         ORDER BY created_at DESC, id LIMIT $4",
    )
    .bind(t.as_uuid())
    .bind(user)
    .bind(unread_only)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        rows.iter()
            .map(|r| Notification {
                id: r.get("id"),
                event_type: r.get("event_type"),
                title: r.get("title"),
                body: r.get("body"),
                link: r.get("link"),
                read: r.get("read"),
                created_at: r.get("created_at"),
            })
            .collect(),
        unread,
    ))
}

/// Tandai dibaca (satu atau semua milik pengguna). Notifikasi orang lain tidak tersentuh.
pub async fn mark_read(
    pool: &PgPool,
    t: TenantId,
    user: Uuid,
    id: Option<Uuid>,
) -> Result<u64, sqlx::Error> {
    let mut tx = tenant_tx(pool, t).await?;
    let n = sqlx::query("UPDATE notifications SET read_at = now() WHERE tenant_id = $1 AND user_id = $2 AND ($3::uuid IS NULL OR id = $3) AND read_at IS NULL")
        .bind(t.as_uuid())
        .bind(user)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(n)
}
