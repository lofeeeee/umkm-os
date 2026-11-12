//! P4-2: approval engine (AC-APR-001..003): 6 tipe entity, multi-step berurutan, timeout,
//! delegasi, anti-bypass pada endpoint langsung, eksekusi setelah persetujuan terakhir.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_domain::approvals::{ENTITY_TYPES, Step};
use umkm_infrastructure::{
    approval_repo::{self as repo, ApprovalRepoError, NewRequest, Policy},
    rbac_repo,
};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

fn base(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}")
}

async fn call(a: &TestApp, m: Method, url: String, who: &Who, body: Option<Value>) -> Res {
    json(&a.router, m, &url, Some(who), body).await
}

async fn role(a: &TestApp, t: Uuid, who: &Who, name: &str) -> String {
    let r = call(a, Method::GET, format!("{}/roles", base(t)), who, None).await;
    r.json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == name)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn policy(a: &TestApp, t: Uuid, o: &Who, entity: &str, steps: Value) -> Res {
    call(
        a,
        Method::PUT,
        format!("{}/approval-policies/{entity}", base(t)),
        o,
        Some(json!({"enabled": true, "steps": steps, "timeout_hours": 24})),
    )
    .await
}

async fn decide(
    a: &TestApp,
    t: Uuid,
    who: &Who,
    id: &str,
    verb: &str,
    step: usize,
    reason: Option<&str>,
) -> Res {
    call(
        a,
        Method::POST,
        format!("{}/approvals/{id}/{verb}", base(t)),
        who,
        Some(json!({"step": step, "reason": reason})),
    )
    .await
}

fn reason(r: &Res) -> Value {
    r.json()["error"]["details"]["reason"].clone()
}

/// Produk aktif + stok 10. Kembalikan (product_id, variant_id).
async fn product(a: &TestApp, t: Uuid, o: &Who) -> (String, String) {
    let p = call(
        a,
        Method::POST,
        format!("{}/products", base(t)),
        o,
        Some(json!({"name": "Kopi", "variants": [{"sku": "K-1", "price": "15000"}]})),
    )
    .await
    .json()["data"]
        .clone();
    let (pid, vid) = (
        p["id"].as_str().unwrap().to_owned(),
        p["variants"][0]["id"].as_str().unwrap().to_owned(),
    );
    call(
        a,
        Method::POST,
        format!("{}/inventory/{vid}/movements", base(t)),
        o,
        Some(json!({"kind": "stock_in", "quantity": 10, "reason": "awal"})),
    )
    .await;
    (pid, vid)
}

async fn on_hand(a: &TestApp, v: &str) -> i32 {
    sqlx::query_scalar("SELECT on_hand FROM inventory_levels WHERE variant_id = $1::uuid")
        .bind(v)
        .fetch_one(&a.owner)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn all_six_entity_types_can_request_approval(pool: PgPool) {
    // AC-APR-001
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let db = a.state.db.clone().unwrap();
    let actor = rbac_repo::load_actor(&db, o.user, TenantId::from_uuid(t))
        .await
        .unwrap()
        .unwrap();
    let r = repo::create(
        &db,
        &actor,
        NewRequest {
            entity_type: ENTITY_TYPES[0],
            entity_id: None,
            action: "order.approve",
            payload: json!({}),
            summary: "x",
        },
        (None, None),
    )
    .await;
    assert!(
        matches!(r, Err(ApprovalRepoError::NoPolicy)),
        "tanpa kebijakan aktif"
    );
    let list = call(
        &a,
        Method::GET,
        "/api/v1/approval-entity-types".into(),
        &o,
        None,
    )
    .await;
    assert_eq!(list.json()["data"].as_array().unwrap().len(), 6);
    for e in ENTITY_TYPES {
        repo::put_policy(
            &db,
            &actor,
            &Policy {
                entity_type: e,
                enabled: true,
                steps: vec![Step {
                    role_ids: vec![],
                    user_ids: vec![],
                }],
                timeout_hours: 24,
            },
            (None, None),
        )
        .await
        .unwrap();
        let id = repo::create(
            &db,
            &actor,
            NewRequest {
                entity_type: e,
                entity_id: Some(Uuid::new_v4()),
                action: "test.request",
                payload: json!({"k": 1}),
                summary: e.as_str(),
            },
            (None, None),
        )
        .await
        .unwrap();
        let v = call(
            &a,
            Method::GET,
            format!("{}/approvals/{id}", base(t)),
            &o,
            None,
        )
        .await;
        assert_eq!(
            (
                v.json()["data"]["entity_type"].clone(),
                v.json()["data"]["status"].clone()
            ),
            (json!(e.as_str()), json!("pending"))
        );
    }
    assert_eq!(
        call(
            &a,
            Method::GET,
            format!("{}/approvals?status=pending", base(t)),
            &o,
            None
        )
        .await
        .json()["data"]["total"],
        6
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sequential_steps_assignment_and_execution(pool: PgPool) {
    // AC-APR-002, AC-APR-003 (stock_adjustment)
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let mgr = member(&pool, t, "Manager").await;
    let cat = member(&pool, t, "Catalog Manager").await;
    let (_, v) = product(&a, t, &o).await;
    let manager_role = role(&a, t, &o, "Manager").await;
    let r = policy(
        &a,
        t,
        &o,
        "stock_adjustment",
        json!([{"role_ids": [manager_role]}, {"user_ids": [o.user]}]),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());

    // Catalog Manager mengurangi stok → 202, stok TIDAK berubah
    let r = call(
        &a,
        Method::POST,
        format!("{}/inventory/{v}/movements", base(t)),
        &cat,
        Some(json!({"kind": "stock_out", "quantity": 3, "reason": "rusak"})),
    )
    .await;
    assert_eq!(r.status, 202, "{}", r.json());
    let id = r.json()["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(on_hand(&a, &v).await, 10);
    // permintaan kedua untuk entity+aksi yang sama → 409 approval_pending
    let r2 = call(
        &a,
        Method::POST,
        format!("{}/inventory/{v}/movements", base(t)),
        &cat,
        Some(json!({"kind": "stock_out", "quantity": 1, "reason": "x"})),
    )
    .await;
    assert_eq!(reason(&r2), json!("approval_pending"));

    // langkah 2 sebelum langkah 1 → 409; Owner tidak ditugaskan di langkah 1 → 403
    assert_eq!(
        reason(&decide(&a, t, &o, &id, "approve", 1, None).await),
        json!("wrong_step")
    );
    assert_eq!(
        reason(&decide(&a, t, &o, &id, "approve", 0, None).await),
        json!("not_assigned")
    );
    assert_eq!(
        decide(&a, t, &cat, &id, "approve", 0, None).await.status,
        403,
        "Catalog Manager tanpa approvals.decide"
    );
    let r = decide(&a, t, &mgr, &id, "approve", 0, None).await;
    assert_eq!(
        (
            r.json()["data"]["status"].clone(),
            r.json()["data"]["current_step"].clone()
        ),
        (json!("pending"), json!(1))
    );
    assert_eq!(
        on_hand(&a, &v).await,
        10,
        "belum diterapkan sebelum langkah terakhir"
    );
    assert_eq!(
        reason(&decide(&a, t, &mgr, &id, "approve", 1, None).await),
        json!("not_assigned")
    );
    let r = decide(&a, t, &o, &id, "approve", 1, None).await;
    assert_eq!(r.json()["data"]["status"], "approved");
    assert!(r.json()["data"]["executed_at"].is_string(), "{}", r.json());
    assert_eq!(
        on_hand(&a, &v).await,
        7,
        "diterapkan sekali setelah persetujuan terakhir"
    );
    let actor: Uuid = sqlx::query_scalar("SELECT actor_user_id FROM inventory_movements WHERE variant_id = $1::uuid AND kind = 'stock_out'")
        .bind(&v).fetch_one(&a.owner).await.unwrap();
    assert_eq!(actor, cat.user, "diterapkan sebagai pemohon");
    let ds: Vec<String> = r.json()["data"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["decision"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ds, ["requested", "approved", "approved", "executed"]);
    assert_eq!(
        reason(&decide(&a, t, &o, &id, "approve", 1, None).await),
        json!("not_pending")
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn direct_changes_cannot_bypass_required_approval(pool: PgPool) {
    // AC-APR-003: refund, price_change, business_config, order
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let mgr = member(&pool, t, "Manager").await;
    let (pid, vid) = product(&a, t, &o).await;
    for e in ["price_change", "business_config", "refund", "order"] {
        assert_eq!(policy(&a, t, &o, e, json!([{}])).await.status, 200);
    }
    // perubahan harga → 202, harga tetap; perubahan non-harga → langsung
    let body = |price: &str, name: &str| json!({"name": name, "variants": [{"id": vid, "sku": "K-1", "price": price}]});
    let r = call(
        &a,
        Method::PUT,
        format!("{}/products/{pid}", base(t)),
        &o,
        Some(body("20000", "Kopi")),
    )
    .await;
    assert_eq!(r.status, 202, "{}", r.json());
    let price_req = r.json()["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = call(
        &a,
        Method::PUT,
        format!("{}/products/{pid}", base(t)),
        &o,
        Some(body("15000", "Kopi Susu")),
    )
    .await;
    assert_eq!(
        (r.status, r.json()["data"]["name"].clone()),
        (200, json!("Kopi Susu"))
    );
    // varian baru juga harga baru → tidak boleh lewat jalur langsung
    let r = call(&a, Method::PUT, format!("{}/products/{pid}", base(t)), &o, Some(json!({"name": "Kopi Susu", "options": [{"name": "U", "values": ["S", "L"]}],
        "variants": [{"id": vid, "sku": "K-1", "option_values": ["S"], "price": "15000"}, {"sku": "K-2", "option_values": ["L"], "price": "1"}]}))).await;
    assert_eq!(
        reason(&r),
        json!("approval_pending"),
        "satu permintaan harga tertunda per produk"
    );
    // pengaturan toko (QRIS) → 202
    let r = call(
        &a,
        Method::PUT,
        format!("{}/commerce/settings", base(t)),
        &o,
        Some(json!({"delivery_fee": "99000"})),
    )
    .await;
    assert_eq!(r.status, 202);
    let cfg_req = r.json()["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fee = call(
        &a,
        Method::GET,
        format!("{}/commerce/settings", base(t)),
        &o,
        None,
    )
    .await
    .json()["data"]["delivery_fee"]
        .clone();
    assert_eq!(fee, "0.00");

    // Manager menyetujui → diterapkan sebagai Owner (pemohon)
    let r = decide(&a, t, &mgr, &price_req, "approve", 0, None).await;
    assert!(r.json()["data"]["executed_at"].is_string(), "{}", r.json());
    let price: String =
        sqlx::query_scalar("SELECT price::text FROM product_variants WHERE id = $1::uuid")
            .bind(&vid)
            .fetch_one(&a.owner)
            .await
            .unwrap();
    assert_eq!(price, "20000.00");
    decide(&a, t, &mgr, &cfg_req, "approve", 0, None).await;
    let fee = call(
        &a,
        Method::GET,
        format!("{}/commerce/settings", base(t)),
        &o,
        None,
    )
    .await
    .json()["data"]["delivery_fee"]
        .clone();
    assert_eq!(fee, "99000.00");
    // pemohon tidak boleh menyetujui permintaannya sendiri
    let r = call(
        &a,
        Method::PUT,
        format!("{}/commerce/settings", base(t)),
        &o,
        Some(json!({"delivery_fee": "1"})),
    )
    .await;
    let own = r.json()["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        reason(&decide(&a, t, &o, &own, "approve", 0, None).await),
        json!("self_approval")
    );
    // menolak wajib alasan; ditolak → tidak diterapkan
    assert_eq!(
        decide(&a, t, &mgr, &own, "reject", 0, None).await.status,
        422
    );
    let r = decide(&a, t, &mgr, &own, "reject", 0, Some("Ongkir terlalu murah")).await;
    assert_eq!(r.json()["data"]["status"], "rejected");
    let fee = call(
        &a,
        Method::GET,
        format!("{}/commerce/settings", base(t)),
        &o,
        None,
    )
    .await
    .json()["data"]["delivery_fee"]
        .clone();
    assert_eq!(fee, "99000.00");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn execution_uses_requester_current_permissions(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let mgr = member(&pool, t, "Manager").await;
    let (_, v) = product(&a, t, &o).await;
    policy(&a, t, &o, "stock_adjustment", json!([{}])).await;
    let r = call(
        &a,
        Method::POST,
        format!("{}/inventory/{v}/movements", base(t)),
        &mgr,
        Some(json!({"kind": "stock_out", "quantity": 2, "reason": "x"})),
    )
    .await;
    let id = r.json()["data"]["approval_request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    // hak pemohon dicabut sebelum persetujuan
    let viewer = role(&a, t, &o, "Viewer").await;
    let mm: String = call(&a, Method::GET, format!("{}/members", base(t)), &o, None)
        .await
        .json()["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user_id"] == mgr.user.to_string())
        .unwrap()["membership_id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &a,
        Method::PATCH,
        format!("{}/members/{mm}", base(t)),
        &o,
        Some(json!({"role_id": viewer})),
    )
    .await;
    let r = decide(&a, t, &o, &id, "approve", 0, None).await;
    assert_eq!(r.json()["data"]["status"], "approved");
    assert!(
        r.json()["data"]["execution_error"]
            .as_str()
            .unwrap()
            .contains("inventory.adjust"),
        "{}",
        r.json()
    );
    assert_eq!(on_hand(&a, &v).await, 10);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn timeout_delegation_cancel_and_isolation(pool: PgPool) {
    let a = app(&pool).await;
    let (t, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let o = member(&pool, t, "Owner").await;
    let ob = member(&pool, tb, "Owner").await;
    let mgr = member(&pool, t, "Manager").await;
    let mgr2 = member(&pool, t, "Manager").await;
    let cat = member(&pool, t, "Catalog Manager").await;
    let (_, v) = product(&a, t, &o).await;
    policy(
        &a,
        t,
        &o,
        "stock_adjustment",
        json!([{"user_ids": [mgr.user]}]),
    )
    .await;
    let req = |n: i64| {
        let (a, v, cat) = (&a, v.clone(), &cat);
        async move {
            let r = call(
                a,
                Method::POST,
                format!("{}/inventory/{v}/movements", base(t)),
                cat,
                Some(json!({"kind": "stock_in", "quantity": n, "reason": "x"})),
            )
            .await;
            r.json()["data"]["approval_request_id"]
                .as_str()
                .unwrap()
                .to_owned()
        }
    };
    // timeout: job maupun keputusan setelah batas → expired / not_pending
    let id = req(1).await;
    sqlx::query(
        "UPDATE approval_requests SET expires_at = now() - interval '1 minute' WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(&a.owner)
    .await
    .unwrap();
    assert_eq!(
        reason(&decide(&a, t, &mgr, &id, "approve", 0, None).await),
        json!("not_pending"),
        "lewat batas walau job belum jalan"
    );
    assert_eq!(
        repo::expire(&a.state.db.clone().unwrap(), 100)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        call(
            &a,
            Method::GET,
            format!("{}/approvals/{id}", base(t)),
            &o,
            None
        )
        .await
        .json()["data"]["status"],
        "expired"
    );

    // delegasi: mgr2 tidak ditugaskan → 403; mgr mendelegasikan → mgr2 boleh atas nama mgr
    let id = req(2).await;
    assert_eq!(
        reason(&decide(&a, t, &mgr2, &id, "approve", 0, None).await),
        json!("not_assigned")
    );
    assert_eq!(
        call(
            &a,
            Method::POST,
            format!("{}/approval-delegations", base(t)),
            &mgr,
            Some(json!({"to_user": mgr2.user, "hours": 24}))
        )
        .await
        .status,
        201
    );
    let r = decide(&a, t, &mgr2, &id, "approve", 0, None).await;
    assert_eq!(r.json()["data"]["status"], "approved", "{}", r.json());
    let d = r.json()["data"]["decisions"][1].clone();
    assert_eq!(
        (d["actor_user_id"].clone(), d["on_behalf_user_id"].clone()),
        (json!(mgr2.user.to_string()), json!(mgr.user.to_string()))
    );
    // delegasi dicabut → tidak lagi berlaku
    let del = call(
        &a,
        Method::GET,
        format!("{}/approval-delegations", base(t)),
        &o,
        None,
    )
    .await
    .json()["data"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        call(
            &a,
            Method::DELETE,
            format!("{}/approval-delegations/{del}", base(t)),
            &mgr2,
            None
        )
        .await
        .status,
        404,
        "hanya pemberi"
    );
    assert_eq!(
        call(
            &a,
            Method::DELETE,
            format!("{}/approval-delegations/{del}", base(t)),
            &mgr,
            None
        )
        .await
        .status,
        204
    );
    let id = req(3).await;
    assert_eq!(
        reason(&decide(&a, t, &mgr2, &id, "approve", 0, None).await),
        json!("not_assigned")
    );

    // batal: hanya pemohon
    assert_eq!(
        reason(
            &call(
                &a,
                Method::POST,
                format!("{}/approvals/{id}/cancel", base(t)),
                &mgr,
                None
            )
            .await
        ),
        json!("not_requester")
    );
    assert_eq!(
        call(
            &a,
            Method::POST,
            format!("{}/approvals/{id}/cancel", base(t)),
            &cat,
            None
        )
        .await
        .status,
        204
    );

    // keputusan paralel pada langkah yang sama → tepat satu tercatat
    let id = req(4).await;
    let rs = futures_util::future::join_all(
        (0..5).map(|_| decide(&a, t, &mgr, &id, "approve", 0, None)),
    )
    .await;
    assert_eq!(rs.iter().filter(|r| r.status == 200).count(), 1);
    assert_eq!(
        on_hand(&a, &v).await,
        10 + 2 + 4,
        "permintaan 2 dan 4 diterapkan masing-masing sekali"
    );
    // DB menolak keputusan kedua untuk langkah yang sama (pertahanan kedua di samping FOR UPDATE)
    let e = sqlx::query(
        "INSERT INTO approval_decisions (tenant_id, request_id, step, decision, actor_user_id)
         VALUES ($1, $2::uuid, 0, 'rejected', $3)",
    )
    .bind(t)
    .bind(&id)
    .bind(mgr2.user)
    .execute(&a.owner)
    .await
    .unwrap_err();
    assert_eq!(
        e.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );

    // isolasi: tenant B tidak melihat/memutus; kebijakan dengan role tenant lain ditolak
    assert_eq!(
        call(
            &a,
            Method::GET,
            format!("{}/approvals/{id}", base(tb)),
            &ob,
            None
        )
        .await
        .status,
        404
    );
    assert_eq!(
        decide(&a, tb, &ob, &id, "approve", 0, None).await.status,
        404
    );
    let kasir = call(
        &a,
        Method::POST,
        format!("{}/roles", base(t)),
        &o,
        Some(json!({"name": "Kasir", "permissions": []})),
    )
    .await;
    assert_eq!(kasir.status, 201);
    let kasir = role(&a, t, &o, "Kasir").await;
    assert_eq!(
        policy(&a, tb, &ob, "refund", json!([{"role_ids": [kasir]}]))
            .await
            .status,
        422
    );
    assert_eq!(
        policy(&a, tb, &ob, "refund", json!([{"user_ids": [o.user]}]))
            .await
            .status,
        422,
        "bukan anggota tenant B"
    );
    assert_eq!(policy(&a, t, &o, "bogus", json!([{}])).await.status, 404);
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE tenant_id = $1 AND action LIKE 'approval.%'",
    )
    .bind(t)
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert!(audits >= 8, "{audits}");
}
