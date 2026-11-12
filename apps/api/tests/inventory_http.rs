//! P3-2: inventory ledger + reservasi (AC-INV-001..005), role aplikasi + RLS.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_infrastructure::inventory_repo::{self as repo, InventoryError, ReservationLine};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

fn inv(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}/inventory")
}

/// Produk dengan dua varian; kembalikan (product_id, [variant_id; 2]).
async fn product(a: &TestApp, t: Uuid, who: &Who, sku: &str) -> (String, [Uuid; 2]) {
    let body = json!({
        "name": format!("Produk {sku}"),
        "options": [{"name": "Ukuran", "values": ["S", "L"]}],
        "variants": [
            {"sku": format!("{sku}-s"), "option_values": ["S"], "price": "10000"},
            {"sku": format!("{sku}-l"), "option_values": ["L"], "price": "12000"}
        ]
    });
    let r = json(
        &a.router,
        Method::POST,
        &format!("/api/v1/tenants/{t}/products"),
        Some(who),
        Some(body),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.json());
    let d = r.json()["data"].clone();
    let v = |i: usize| Uuid::parse_str(d["variants"][i]["id"].as_str().unwrap()).unwrap();
    (d["id"].as_str().unwrap().to_owned(), [v(0), v(1)])
}

async fn mv(a: &TestApp, t: Uuid, who: &Who, v: Uuid, kind: &str, qty: i64, reason: &str) -> Res {
    json(
        &a.router,
        Method::POST,
        &format!("{}/{v}/movements", inv(t)),
        Some(who),
        Some(json!({"kind": kind, "quantity": qty, "reason": reason, "reference": "PO-7"})),
    )
    .await
}

async fn settings(a: &TestApp, t: Uuid, who: &Who, v: Uuid, min: Option<i64>, neg: bool) -> Res {
    json(
        &a.router,
        Method::PUT,
        &format!("{}/{v}/settings", inv(t)),
        Some(who),
        Some(json!({"min_stock": min, "allow_negative": neg})),
    )
    .await
}

async fn low_events(a: &TestApp, v: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM notification_outbox WHERE event_type = 'inventory.low_stock' AND payload->>'variant_id' = $1",
    )
    .bind(v.to_string())
    .fetch_one(&a.owner)
    .await
    .unwrap()
}

fn line(v: Uuid, q: i64) -> ReservationLine {
    ReservationLine {
        variant_id: v,
        quantity: q,
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn movements_are_complete_and_immutable(pool: PgPool) {
    // AC-INV-001
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let (_, [v, _]) = product(&a, t, &o, "kp").await;
    assert_eq!(
        mv(&a, t, &o, v, "stock_in", 10, "Belanja pasar")
            .await
            .status,
        201
    );
    assert_eq!(mv(&a, t, &o, v, "stock_out", 3, "Rusak").await.status, 201);
    let r = mv(&a, t, &o, v, "adjustment", 5, "Stock opname").await;
    assert_eq!(r.json()["data"]["on_hand"], 5);
    assert_eq!(
        mv(&a, t, &o, v, "adjustment", 5, "Opname sama")
            .await
            .status,
        422,
        "delta 0"
    );
    assert_eq!(
        mv(&a, t, &o, v, "sale", 1, "x").await.status,
        422,
        "sale hanya dari order"
    );
    assert_eq!(mv(&a, t, &o, v, "stock_in", 0, "x").await.status, 422);
    assert_eq!(
        mv(&a, t, &o, v, "stock_in", 1, " ").await.status,
        422,
        "alasan wajib"
    );

    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/{v}/movements", inv(t)),
        Some(&o),
        None,
    )
    .await;
    let list = r.json()["data"].as_array().unwrap().clone();
    let got: Vec<(String, i64, i64)> = list
        .iter()
        .map(|m| {
            (
                m["kind"].as_str().unwrap().into(),
                m["delta"].as_i64().unwrap(),
                m["on_hand_after"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("adjustment".into(), -2, 5),
            ("stock_out".into(), -3, 7),
            ("stock_in".into(), 10, 10)
        ]
    );
    for m in &list {
        assert_eq!(m["actor_user_id"], o.user.to_string());
        assert_eq!(m["reference"], "PO-7");
        assert!(m["reason"].as_str().is_some_and(|s| !s.is_empty()) && m["created_at"].is_string());
    }
    let tenant_ok: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inventory_movements WHERE tenant_id = $1 AND variant_id = $2",
    )
    .bind(t)
    .bind(v)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(tenant_ok, 3);

    // Immutable: role aplikasi tidak bisa UPDATE/DELETE movement.
    let app_db = app_pool(&pool).await;
    for sql in [
        "UPDATE inventory_movements SET delta = 999 WHERE tenant_id = $1",
        "DELETE FROM inventory_movements WHERE tenant_id = $1",
    ] {
        let mut tx = umkm_database::begin_tenant_tx(&app_db, TenantId::from_uuid(t))
            .await
            .unwrap();
        let e = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(t)
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(
            e.as_database_error().unwrap().code().as_deref(),
            Some("42501"),
            "{sql}"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn low_stock_event_once_per_crossing(pool: PgPool) {
    // AC-INV-002
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let (_, [v, _]) = product(&a, t, &o, "kp").await;
    mv(&a, t, &o, v, "stock_in", 10, "awal").await;
    assert_eq!(settings(&a, t, &o, v, Some(5), false).await.status, 200);
    assert_eq!(low_events(&a, v).await, 0);
    let r = mv(&a, t, &o, v, "stock_out", 6, "jual offline").await;
    assert_eq!(r.json()["data"]["is_low"], true);
    assert_eq!(low_events(&a, v).await, 1);
    mv(&a, t, &o, v, "stock_out", 1, "jual offline").await;
    assert_eq!(
        low_events(&a, v).await,
        1,
        "masih di bawah ambang: tidak ada event baru"
    );
    mv(&a, t, &o, v, "stock_in", 10, "restock").await;
    mv(&a, t, &o, v, "stock_out", 9, "jual").await;
    assert_eq!(low_events(&a, v).await, 2, "melintasi lagi setelah pulih");
    // Reservasi juga dapat melintasi ambang
    mv(&a, t, &o, v, "stock_in", 10, "restock").await;
    repo::reserve(
        &a.state.db.clone().unwrap(),
        TenantId::from_uuid(t),
        &[line(v, 10)],
        "order:x",
        600,
    )
    .await
    .unwrap();
    assert_eq!(low_events(&a, v).await, 3);
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}?low=true", inv(t)),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(r.json()["data"]["total"], 1);
    assert_eq!(r.json()["data"]["items"][0]["variant_id"], v.to_string());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn reservation_expiry_release_and_commit(pool: PgPool) {
    // AC-INV-003
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let (_, [v1, v2]) = product(&a, t, &o, "kp").await;
    mv(&a, t, &o, v1, "stock_in", 5, "awal").await;
    mv(&a, t, &o, v2, "stock_in", 5, "awal").await;
    let db = a.state.db.clone().unwrap();
    let tid = TenantId::from_uuid(t);
    let avail = |v: Uuid| {
        let db = db.clone();
        async move { repo::level(&db, tid, v).await.unwrap().unwrap().available() }
    };

    // kedaluwarsa → tersedia kembali; job idempoten
    repo::reserve(
        &db,
        tid,
        &[line(v1, 2), line(v2, 1), line(v1, 1)],
        "order:a",
        0,
    )
    .await
    .unwrap();
    assert_eq!(
        (avail(v1).await, avail(v2).await),
        (2, 4),
        "baris varian sama digabung"
    );
    assert_eq!(repo::expire_reservations(&db, 100).await.unwrap(), 2);
    assert_eq!(repo::expire_reservations(&db, 100).await.unwrap(), 0);
    assert_eq!((avail(v1).await, avail(v2).await), (5, 5));
    assert_eq!(
        repo::commit(&db, tid, "order:a", None).await.unwrap(),
        0,
        "reservasi kedaluwarsa tidak bisa dijual"
    );

    // reservasi yang belum kedaluwarsa tidak disentuh job
    repo::reserve(&db, tid, &[line(v1, 3)], "order:b", 600)
        .await
        .unwrap();
    assert_eq!(repo::expire_reservations(&db, 100).await.unwrap(), 0);
    assert_eq!(avail(v1).await, 2);

    // release idempoten
    assert_eq!(repo::release(&db, tid, "order:b").await.unwrap(), 1);
    assert_eq!(repo::release(&db, tid, "order:b").await.unwrap(), 0);
    assert_eq!(avail(v1).await, 5);

    // commit: stok fisik turun, movement sale, idempoten
    repo::reserve(&db, tid, &[line(v1, 2)], "order:c", 600)
        .await
        .unwrap();
    assert_eq!(
        repo::commit(&db, tid, "order:c", Some(o.user))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        repo::commit(&db, tid, "order:c", Some(o.user))
            .await
            .unwrap(),
        0
    );
    let l = repo::level(&db, tid, v1).await.unwrap().unwrap();
    assert_eq!((l.on_hand, l.reserved), (3, 0));
    let sale: (i32, String) = sqlx::query_as(
        "SELECT delta, reference FROM inventory_movements WHERE variant_id = $1 AND kind = 'sale'",
    )
    .bind(v1)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(sale, (-2, "order:c".into()));

    // all-or-nothing: satu baris kurang → tidak ada reservasi sama sekali
    let e = repo::reserve(&db, tid, &[line(v2, 1), line(v1, 99)], "order:d", 600)
        .await
        .unwrap_err();
    assert!(matches!(e, InventoryError::Stock(_)), "{e:?}");
    assert_eq!(avail(v2).await, 5);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inventory_reservations WHERE reference = 'order:d'",
    )
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn negative_stock_disabled_by_default(pool: PgPool) {
    // AC-INV-004
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let (_, [v, _]) = product(&a, t, &o, "kp").await;
    mv(&a, t, &o, v, "stock_in", 2, "awal").await;
    let r = mv(&a, t, &o, v, "stock_out", 3, "jual").await;
    assert_eq!(r.status, 409);
    assert_eq!(r.json()["error"]["details"]["reason"], "insufficient_stock");
    assert_eq!(r.json()["error"]["details"]["available"], 2);
    let db = a.state.db.clone().unwrap();
    let tid = TenantId::from_uuid(t);
    assert!(
        repo::reserve(&db, tid, &[line(v, 3)], "order:x", 600)
            .await
            .is_err()
    );

    // izin eksplisit → boleh negatif; mematikan lagi saat negatif → 409
    assert_eq!(settings(&a, t, &o, v, None, true).await.status, 200);
    assert_eq!(
        mv(&a, t, &o, v, "stock_out", 3, "jual").await.json()["data"]["available"],
        -1
    );
    let r = settings(&a, t, &o, v, None, false).await;
    assert_eq!(
        r.json()["error"]["details"]["reason"],
        "negative_stock_exists"
    );
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE tenant_id = $1 AND action = 'inventory.settings_changed'",
    )
    .bind(t)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(audits, 1, "hanya perubahan yang berhasil diaudit");

    // Pertahanan terakhir DB: UPDATE langsung sebagai role aplikasi melanggar CHECK.
    let (_, [w, _]) = product(&a, t, &o, "kq").await;
    let app_db = app_pool(&pool).await;
    let mut tx = umkm_database::begin_tenant_tx(&app_db, tid).await.unwrap();
    let e = sqlx::query(
        "UPDATE inventory_levels SET on_hand = -1 WHERE tenant_id = $1 AND variant_id = $2",
    )
    .bind(t)
    .bind(w)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(
        e.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn parallel_checkouts_never_oversell(pool: PgPool) {
    // AC-INV-005 (fixture kebenaran; bukan angka benchmark PRD)
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let (_, [v1, v2]) = product(&a, t, &o, "kp").await;
    let db = a.state.db.clone().unwrap();
    let tid = TenantId::from_uuid(t);
    for (stock, n) in [(1_i64, 20_usize), (5, 20)] {
        let (_, [v, _]) = product(&a, t, &o, &format!("p{stock}")).await;
        mv(&a, t, &o, v, "stock_in", stock, "awal").await;
        let rs =
            futures_util::future::join_all(
                (0..n).map(|i| {
                    let db = db.clone();
                    async move {
                        repo::reserve(&db, tid, &[line(v, 1)], &format!("order:{i}"), 600).await
                    }
                }),
            )
            .await;
        let ok = rs.iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            ok as i64, stock,
            "stok {stock}: tepat {stock} checkout berhasil"
        );
        assert!(
            rs.iter()
                .filter_map(|r| r.as_ref().err())
                .all(|e| matches!(e, InventoryError::Stock(_)))
        );
        let l = repo::level(&db, tid, v).await.unwrap().unwrap();
        assert_eq!((l.on_hand, l.reserved, l.available()), (stock, stock, 0));
    }
    // Urutan baris berlawanan secara paralel: tidak deadlock (kunci urut variant_id).
    mv(&a, t, &o, v1, "stock_in", 100, "awal").await;
    mv(&a, t, &o, v2, "stock_in", 100, "awal").await;
    let rs = futures_util::future::join_all((0..20).map(|i| {
        let db = db.clone();
        let lines = if i % 2 == 0 {
            vec![line(v1, 1), line(v2, 1)]
        } else {
            vec![line(v2, 1), line(v1, 1)]
        };
        async move { repo::reserve(&db, tid, &lines, &format!("mix:{i}"), 600).await }
    }))
    .await;
    assert!(
        rs.iter().all(Result::is_ok),
        "{:?}",
        rs.iter()
            .filter_map(|r| r.as_ref().err())
            .collect::<Vec<_>>()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn permissions_isolation_and_variant_in_use(pool: PgPool) {
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let o = member(&pool, ta, "Owner").await;
    let viewer = member(&pool, ta, "Viewer").await;
    let catalog = member(&pool, ta, "Catalog Manager").await;
    let ob = member(&pool, tb, "Owner").await;
    let (pid, [v, _]) = product(&a, ta, &o, "kp").await;

    assert_eq!(mv(&a, ta, &viewer, v, "stock_in", 1, "x").await.status, 403);
    assert_eq!(
        settings(&a, ta, &viewer, v, Some(1), false).await.status,
        403
    );
    assert_eq!(
        mv(&a, ta, &catalog, v, "stock_in", 4, "x").await.status,
        201
    );
    let r = json(&a.router, Method::GET, &inv(ta), Some(&viewer), None).await;
    assert_eq!(r.json()["data"]["total"], 2);
    assert_eq!(r.json()["data"]["items"][0]["on_hand"], 4);

    // Tenant B tidak melihat/mengubah varian tenant A
    assert_eq!(mv(&a, tb, &ob, v, "stock_in", 1, "x").await.status, 404);
    assert_eq!(settings(&a, tb, &ob, v, Some(1), true).await.status, 404);
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/{v}/movements", inv(tb)),
        Some(&ob),
        None,
    )
    .await;
    assert_eq!(r.status, 404);
    assert_eq!(
        json(&a.router, Method::GET, &inv(tb), Some(&ob), None)
            .await
            .json()["data"]["total"],
        0
    );
    assert!(
        repo::reserve(
            &a.state.db.clone().unwrap(),
            TenantId::from_uuid(tb),
            &[line(v, 1)],
            "o",
            60
        )
        .await
        .is_err()
    );

    // Varian dengan riwayat stok tidak dapat dihapus dari produk
    let body: Value = json!({"name": "Produk kp", "variants": [{"sku": "kp-new", "price": "1"}]});
    let r = json(
        &a.router,
        Method::PUT,
        &format!("/api/v1/tenants/{ta}/products/{pid}"),
        Some(&o),
        Some(body),
    )
    .await;
    assert_eq!(r.status, 409, "{}", r.json());
    assert_eq!(r.json()["error"]["details"]["reason"], "variant_in_use");
}
