//! Eksekutor tool nyata (P5-6) untuk SATU tenant: setiap query di transaksi RLS tenant itu.
//! Hanya data publik toko atau milik pelanggan terverifikasi (customer_id dari konteks percakapan).
//! `create_order_draft` TIDAK menulis apa pun: hanya menghitung draft harga & ketersediaan; pesanan
//! tetap dibuat pelanggan lewat checkout (pembayaran/refund tidak pernah dapat dipicu AI).

use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_application::ai::{
    orchestrator::Retriever,
    tools::{CustomerCtx, ToolArgs, ToolBackend, ToolError},
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

use crate::pgx::tenant_tx;

/// (label, alamat, lintang, bujur)
type AddressRow = (Option<String>, String, Option<f64>, Option<f64>);

pub struct PgToolBackend<'a, R: Retriever> {
    pub pool: &'a PgPool,
    pub tenant: TenantId,
    /// Untuk `search_faq`; `None` = tenant tanpa basis pengetahuan.
    pub faq: Option<&'a R>,
}

fn like(q: &str) -> String {
    format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

/// Jumlah stok yang diungkap ke pelanggan dibatasi (tidak membocorkan persediaan persis bila besar).
fn shown_qty(n: i64) -> Value {
    if n > 99 {
        json!("99+")
    } else {
        json!(n.max(0))
    }
}

impl<R: Retriever> PgToolBackend<'_, R> {
    async fn run(
        &self,
        args: &ToolArgs,
        customer: Option<CustomerCtx>,
    ) -> Result<Value, sqlx::Error> {
        let mut tx = tenant_tx(self.pool, self.tenant).await?;
        let v = match args {
            ToolArgs::SearchProducts { query, limit } => {
                let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
                    "SELECT p.id, p.name,
                            (SELECT min(coalesce(v.promo_price, v.price))::text FROM product_variants v
                              WHERE v.product_id = p.id AND v.is_active)
                     FROM products p
                     WHERE p.status = 'active' AND p.available
                       AND (p.name ILIKE $1 ESCAPE '\\' OR coalesce(p.description, '') ILIKE $1 ESCAPE '\\')
                     ORDER BY p.name LIMIT $2",
                )
                .bind(like(query))
                .bind(i64::from(*limit))
                .fetch_all(&mut *tx)
                .await?;
                json!({ "products": rows.into_iter().map(|(id, name, price)| json!({ "product_id": id, "name": name, "price_from": price })).collect::<Vec<_>>() })
            }
            ToolArgs::GetProductDetail { product_id } => {
                let p: Option<(String, Option<String>)> = sqlx::query_as(
                    "SELECT name, left(description, 500) FROM products WHERE id = $1 AND status = 'active'",
                )
                .bind(product_id)
                .fetch_optional(&mut *tx)
                .await?;
                match p {
                    None => json!({ "found": false }),
                    Some((name, description)) => {
                        let vars: Vec<(String, String, Option<String>)> = sqlx::query_as(
                            "SELECT name, price::text, promo_price::text FROM product_variants
                             WHERE product_id = $1 AND is_active ORDER BY position LIMIT 20",
                        )
                        .bind(product_id)
                        .fetch_all(&mut *tx)
                        .await?;
                        json!({ "found": true, "name": name, "description": description,
                            "variants": vars.into_iter().map(|(n, p, promo)| json!({ "name": n, "price": p, "promo_price": promo })).collect::<Vec<_>>() })
                    }
                }
            }
            ToolArgs::CheckStock { product_id } => {
                let rows: Vec<(String, i64)> = sqlx::query_as(
                    "SELECT v.name, coalesce(l.on_hand - l.reserved, 0)::int8
                     FROM product_variants v JOIN products p ON p.id = v.product_id AND p.status = 'active'
                     LEFT JOIN inventory_levels l ON l.variant_id = v.id
                     WHERE v.product_id = $1 AND v.is_active ORDER BY v.position LIMIT 20",
                )
                .bind(product_id)
                .fetch_all(&mut *tx)
                .await?;
                if rows.is_empty() {
                    json!({ "found": false })
                } else {
                    json!({ "found": true, "variants": rows.into_iter().map(|(n, q)| json!({ "name": n, "in_stock": q > 0, "available": shown_qty(q) })).collect::<Vec<_>>() })
                }
            }
            ToolArgs::GetBusinessHours => {
                let tz: Option<String> =
                    sqlx::query_scalar("SELECT timezone FROM business_profiles")
                        .fetch_optional(&mut *tx)
                        .await?;
                let rows: Vec<(i16, String, String)> = sqlx::query_as(
                    "SELECT day_of_week, to_char(open_time, 'HH24:MI'), to_char(close_time, 'HH24:MI')
                     FROM business_hours ORDER BY day_of_week, open_time",
                )
                .fetch_all(&mut *tx)
                .await?;
                json!({ "timezone": tz, "hours": rows.into_iter().map(|(d, o, c)| json!({ "day_of_week": d, "open": o, "close": c })).collect::<Vec<_>>(),
                        "note": "day_of_week: 1 = Senin … 7 = Minggu; hari tanpa baris = tutup" })
            }
            ToolArgs::GetBusinessLocation => {
                let rows: Vec<AddressRow> = sqlx::query_as(
                    "SELECT label, address, latitude, longitude FROM business_addresses ORDER BY position LIMIT 5",
                )
                .fetch_all(&mut *tx)
                .await?;
                let contact: Option<(Option<String>, Option<String>)> =
                    sqlx::query_as("SELECT phone, whatsapp FROM business_profiles")
                        .fetch_optional(&mut *tx)
                        .await?;
                let (phone, whatsapp) = contact.unwrap_or_default();
                json!({ "addresses": rows.into_iter().map(|(l, a, lat, lng)| json!({ "label": l, "address": a, "latitude": lat, "longitude": lng })).collect::<Vec<_>>(),
                        "phone": phone, "whatsapp": whatsapp })
            }
            ToolArgs::SearchFaq { .. } => unreachable!("ditangani di execute"),
            ToolArgs::GetCustomerOrders { limit } => {
                let c = customer
                    .filter(|c| c.verified)
                    .ok_or(sqlx::Error::RowNotFound)?;
                let rows: Vec<(String, String, String, String)> = sqlx::query_as(
                    "SELECT code, status, total::text, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD')
                     FROM orders WHERE customer_id = $1 ORDER BY created_at DESC LIMIT $2",
                )
                .bind(c.customer_id)
                .bind(i64::from(*limit))
                .fetch_all(&mut *tx)
                .await?;
                json!({ "orders": rows.into_iter().map(|(code, st, total, d)| json!({ "order_number": code, "status": st, "total": total, "date": d })).collect::<Vec<_>>() })
            }
            ToolArgs::GetOrderStatus { order_number } => {
                let c = customer
                    .filter(|c| c.verified)
                    .ok_or(sqlx::Error::RowNotFound)?;
                // Bukan milik pelanggan = sama dengan tidak ada (tidak bisa menebak pesanan orang lain).
                let r: Option<(String, String, String)> = sqlx::query_as(
                    "SELECT status, fulfillment, total::text FROM orders WHERE code = $1 AND customer_id = $2",
                )
                .bind(order_number)
                .bind(c.customer_id)
                .fetch_optional(&mut *tx)
                .await?;
                match r {
                    None => json!({ "found": false }),
                    Some((status, fulfillment, total)) => {
                        json!({ "found": true, "order_number": order_number, "status": status, "fulfillment": fulfillment, "total": total })
                    }
                }
            }
            ToolArgs::CreateOrderDraft { items } => {
                let mut lines = vec![];
                let mut problems = vec![];
                for (variant, qty) in items {
                    let r: Option<(String, String, String, i64)> = sqlx::query_as(
                        "SELECT p.name || ' - ' || v.name, coalesce(v.promo_price, v.price)::text,
                                (coalesce(v.promo_price, v.price) * $2)::text,
                                coalesce(l.on_hand - l.reserved, 0)::int8
                         FROM product_variants v JOIN products p ON p.id = v.product_id
                         LEFT JOIN inventory_levels l ON l.variant_id = v.id
                         WHERE v.id = $1 AND v.is_active AND p.status = 'active' AND p.available",
                    )
                    .bind(variant)
                    .bind(i32::from(*qty))
                    .fetch_optional(&mut *tx)
                    .await?;
                    match r {
                        None => problems
                            .push(json!({ "variant_id": variant, "problem": "not_available" })),
                        Some((name, unit, line, avail)) => {
                            if avail < i64::from(*qty) {
                                problems.push(json!({ "variant_id": variant, "problem": "insufficient_stock", "available": shown_qty(avail) }));
                            }
                            lines.push(json!({ "variant_id": variant, "name": name, "quantity": qty, "unit_price": unit, "line_total": line }));
                        }
                    }
                }
                json!({ "draft": true, "items": lines, "problems": problems,
                        "note": "Draft belum menjadi pesanan. Pelanggan menyelesaikan pesanan & pembayaran di halaman checkout toko." })
            }
            ToolArgs::RequestHumanSupport { .. } => json!({ "ok": true }),
        };
        tx.commit().await?;
        Ok(v)
    }
}

impl<R: Retriever> ToolBackend for PgToolBackend<'_, R> {
    async fn execute(
        &self,
        args: &ToolArgs,
        customer: Option<CustomerCtx>,
    ) -> Result<Value, ToolError> {
        if let ToolArgs::SearchFaq { query } = args {
            let Some(r) = self.faq else {
                return Ok(json!({ "results": [] }));
            };
            let hits = r.retrieve(query, 3).await.map_err(|_| ToolError::Failed)?;
            return Ok(json!({ "results": hits }));
        }
        let scoped = matches!(
            args,
            ToolArgs::GetCustomerOrders { .. }
                | ToolArgs::GetOrderStatus { .. }
                | ToolArgs::CreateOrderDraft { .. }
        );
        if scoped && !customer.is_some_and(|c| c.verified) {
            // Pertahanan berlapis: authorize() sudah menolak sebelum sampai sini.
            return Err(ToolError::Denied("customer_not_verified"));
        }
        self.run(args, customer).await.map_err(|e| {
            tracing::warn!(error = %e, "ai tool query failed");
            ToolError::Failed
        })
    }
}
