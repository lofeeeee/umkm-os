//! P3-1: katalog produk (AC-CAT-001..004), role aplikasi + RLS.

mod common;

use axum::http::Method;
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use umkm_database::MIGRATOR;
use uuid::Uuid;

fn products(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}/products")
}

fn categories(t: Uuid) -> String {
    format!("/api/v1/tenants/{t}/categories")
}

fn product(name: &str, sku: &str) -> Value {
    json!({
        "name": name,
        "description": "Kopi susu dengan gula aren",
        "variants": [{"sku": sku, "price": "18000", "promo_price": "15000"}]
    })
}

fn sized(name: &str, skus: [&str; 2]) -> Value {
    json!({
        "name": name,
        "options": [{"name": "Ukuran", "values": ["S", "L"]}],
        "variants": [
            {"sku": skus[0], "option_values": ["S"], "price": "10000"},
            {"sku": skus[1], "option_values": ["L"], "price": "14000.50"}
        ]
    })
}

fn codes(r: &Res) -> Vec<String> {
    r.json()["error"]["details"]["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| {
                    format!(
                        "{}:{}",
                        e["path"].as_str().unwrap(),
                        e["code"].as_str().unwrap()
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn create(a: &TestApp, t: Uuid, who: &Who, body: Value) -> Value {
    let r = json(&a.router, Method::POST, &products(t), Some(who), Some(body)).await;
    assert_eq!(r.status, 201, "{}", r.json());
    r.json()["data"].clone()
}

async fn status(a: &TestApp, t: Uuid, who: &Who, id: &str, to: &str) -> Res {
    json(
        &a.router,
        Method::POST,
        &format!("{}/{id}/status", products(t)),
        Some(who),
        Some(json!({ "status": to })),
    )
    .await
}

async fn call(a: &TestApp, m: Method, url: String, who: &Who, body: Option<Value>) -> Res {
    json(&a.router, m, &url, Some(who), body).await
}

async fn post_cat(a: &TestApp, t: Uuid, who: &Who, body: Value) -> Res {
    json(
        &a.router,
        Method::POST,
        &categories(t),
        Some(who),
        Some(body),
    )
    .await
}

async fn public(a: &TestApp, host: &str, path: &str) -> Res {
    send(&a.router, Method::GET, path, None, Some(host), None, vec![]).await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn lifecycle_status_and_public_visibility(pool: PgPool) {
    // AC-CAT-001
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let p = create(&a, t, &o, product("Kopi Susu", "ks-1")).await;
    let id = p["id"].as_str().unwrap().to_owned();
    assert_eq!(p["status"], "draft");
    assert_eq!(p["slug"], "kopi-susu");
    assert_eq!(
        p["variants"][0]["sku"], "KS-1",
        "SKU dinormalkan huruf besar"
    );
    assert_eq!(p["variants"][0]["price"], "18000.00");
    assert_eq!(p["variants"][0]["promo_price"], "15000.00");

    let host = "toko-a.ryasai.my.id";
    let list = |r: Res| r.json()["data"]["items"].as_array().unwrap().len();
    assert_eq!(
        list(public(&a, host, "/api/v1/public/products").await),
        0,
        "draft tidak publik"
    );
    assert_eq!(
        public(&a, host, "/api/v1/public/products/kopi-susu")
            .await
            .status,
        404
    );

    // draft → archived dilarang; draft → active boleh
    assert_eq!(status(&a, t, &o, &id, "archived").await.status, 409);
    assert_eq!(status(&a, t, &o, &id, "active").await.status, 200);
    let r = public(&a, host, "/api/v1/public/products").await;
    assert_eq!(r.headers["cache-control"], "public, max-age=60");
    let item = &r.json()["data"]["items"][0];
    assert_eq!(item["name"], "Kopi Susu");
    assert!(
        item.get("status").is_none() && item["variants"][0].get("sku").is_none(),
        "{item}"
    );
    assert_eq!(
        public(&a, host, "/api/v1/public/products/kopi-susu")
            .await
            .status,
        200
    );

    // active → draft dilarang; active → archived → tidak publik; archived → active boleh
    assert_eq!(status(&a, t, &o, &id, "draft").await.status, 422);
    assert_eq!(status(&a, t, &o, &id, "archived").await.status, 200);
    assert_eq!(
        list(public(&a, host, "/api/v1/public/products").await),
        0,
        "archived tidak publik"
    );
    assert_eq!(
        public(&a, host, "/api/v1/public/products/kopi-susu")
            .await
            .status,
        404
    );
    assert_eq!(status(&a, t, &o, &id, "active").await.status, 200);

    let audits: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE tenant_id = $1 AND resource_id = $2 ORDER BY created_at, id",
    )
    .bind(t)
    .bind(&id)
    .fetch_all(&a.owner)
    .await
    .unwrap();
    assert_eq!(
        audits,
        [
            "product.created",
            "product.status_changed",
            "product.status_changed",
            "product.status_changed"
        ]
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_status_change_applies_once(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let id = create(&a, t, &o, product("Teh", "teh-1")).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let rs = futures_util::future::join_all((0..6).map(|_| status(&a, t, &o, &id, "active"))).await;
    let ok = rs.iter().filter(|r| r.status == 200).count();
    let conflict = rs.iter().filter(|r| r.status == 409).count();
    assert_eq!(
        (ok, conflict),
        (1, 5),
        "{:?}",
        rs.iter().map(|r| r.status).collect::<Vec<_>>()
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sku_unique_per_tenant(pool: PgPool) {
    // AC-CAT-003
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let (oa, ob) = (
        member(&pool, ta, "Owner").await,
        member(&pool, tb, "Owner").await,
    );
    create(&a, ta, &oa, product("Kopi", "X-1")).await;
    let r = json(
        &a.router,
        Method::POST,
        &products(ta),
        Some(&oa),
        Some(product("Kopi Lain", "x-1")),
    )
    .await;
    assert_eq!(r.status, 409, "{}", r.json());
    assert_eq!(r.json()["error"]["details"]["reason"], "sku_taken");
    // slug sama di tenant yang sama juga bentrok
    let r = json(
        &a.router,
        Method::POST,
        &products(ta),
        Some(&oa),
        Some(product("Kopi", "x-2")),
    )
    .await;
    assert_eq!(r.json()["error"]["details"]["reason"], "slug_taken");
    // tenant B boleh SKU + slug yang sama
    create(&a, tb, &ob, product("Kopi", "X-1")).await;

    // Tukar SKU dua varian dalam satu produk: constraint deferred → boleh
    let p = create(&a, ta, &oa, sized("Es Teh", ["et-s", "et-l"])).await;
    let mut body = sized("Es Teh", ["et-l", "et-s"]);
    body["variants"][0]["id"] = p["variants"][0]["id"].clone();
    body["variants"][1]["id"] = p["variants"][1]["id"].clone();
    let r = json(
        &a.router,
        Method::PUT,
        &format!("{}/{}", products(ta), p["id"].as_str().unwrap()),
        Some(&oa),
        Some(body),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.json());
    assert_eq!(r.json()["data"]["variants"][0]["sku"], "ET-L");
    assert_eq!(
        r.json()["data"]["variants"][0]["id"],
        p["variants"][0]["id"],
        "id varian tetap"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn invalid_prices_rejected_by_api_and_database(pool: PgPool) {
    // AC-CAT-004
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    for (price, promo, code) in [
        (
            "100",
            Some("150"),
            "variants[0].promo_price:promo_not_below_price",
        ),
        (
            "100",
            Some("100"),
            "variants[0].promo_price:promo_not_below_price",
        ),
        ("-1", None, "variants[0].price:invalid_price"),
        ("1e3", None, "variants[0].price:invalid_price"),
        ("1000000000.01", None, "variants[0].price:invalid_price"),
    ] {
        let body =
            json!({"name": "X", "variants": [{"sku": "a", "price": price, "promo_price": promo}]});
        let r = json(&a.router, Method::POST, &products(t), Some(&o), Some(body)).await;
        assert_eq!(r.status, 422);
        assert!(
            codes(&r).contains(&code.to_owned()),
            "{price} {promo:?}: {:?}",
            codes(&r)
        );
    }
    // Negative control: lewati validasi Rust, langsung ke DB sebagai role aplikasi.
    let p = create(&a, t, &o, product("Kopi", "k-1")).await;
    let app_db = app_pool(&pool).await;
    for (price, promo) in [("-1", None), ("100", Some("100")), ("100", Some("150"))] {
        let mut tx =
            umkm_database::begin_tenant_tx(&app_db, umkm_shared::ids::TenantId::from_uuid(t))
                .await
                .unwrap();
        let e = sqlx::query(
            "INSERT INTO product_variants (tenant_id, product_id, sku, name, price, promo_price, position)
             VALUES ($1, $2, 'DB-1', 'x', $3::numeric, $4::numeric, 5)",
        )
        .bind(t)
        .bind(Uuid::parse_str(p["id"].as_str().unwrap()).unwrap())
        .bind(price)
        .bind(promo)
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(
            e.as_database_error().unwrap().code().as_deref(),
            Some("23514"),
            "{price}"
        );
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn category_tree_is_tenant_owned_and_two_levels(pool: PgPool) {
    // AC-CAT-002
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let (oa, ob) = (
        member(&pool, ta, "Owner").await,
        member(&pool, tb, "Owner").await,
    );
    let post = |t: Uuid, w: &'static str, body: Value| {
        post_cat(&a, t, if w == "a" { &oa } else { &ob }, body)
    };
    let minuman = post(ta, "a", json!({"name": "Minuman"})).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let kopi = post(ta, "a", json!({"name": "Kopi", "parent_id": minuman})).await;
    assert_eq!(kopi.status, 201);
    let kopi = kopi.json()["data"]["id"].as_str().unwrap().to_owned();
    let other = post(tb, "b", json!({"name": "Makanan"})).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // parent milik tenant lain → ditolak (composite FK)
    let r = post(ta, "a", json!({"name": "Roti", "parent_id": other})).await;
    assert_eq!(
        (r.status, codes(&r)),
        (422, vec!["parent_id:invalid_parent".to_owned()])
    );
    // tingkat ketiga → ditolak
    let r = post(ta, "a", json!({"name": "Arabika", "parent_id": kopi})).await;
    assert_eq!(codes(&r), vec!["parent_id:invalid_parent".to_owned()]);
    // siklus: induk dijadikan anak dari anaknya → ditolak
    let r = json(
        &a.router,
        Method::PUT,
        &format!("{}/{minuman}", categories(ta)),
        Some(&oa),
        Some(json!({"name": "Minuman", "parent_id": kopi})),
    )
    .await;
    assert_eq!(codes(&r), vec!["parent_id:invalid_parent".to_owned()]);
    // nama sama di induk yang sama → 409
    let r = post(ta, "a", json!({"name": "minuman"})).await;
    assert_eq!(
        r.json()["error"]["details"]["reason"],
        "category_name_taken"
    );

    // produk di kategori tenant lain → ditolak
    let mut body = product("Kopi", "k-1");
    body["category_id"] = json!(other);
    let r = json(
        &a.router,
        Method::POST,
        &products(ta),
        Some(&oa),
        Some(body.clone()),
    )
    .await;
    assert_eq!(codes(&r), vec!["category_id:invalid_category".to_owned()]);
    body["category_id"] = json!(kopi);
    let p = create(&a, ta, &oa, body).await;

    // filter kategori induk mencakup subkategori
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}?category_id={minuman}", products(ta)),
        Some(&oa),
        None,
    )
    .await;
    assert_eq!(r.json()["data"]["total"], 1);

    // hapus induk yang masih punya anak → 409; hapus anak → produk jadi tanpa kategori
    let del = |id: &str| {
        call(
            &a,
            Method::DELETE,
            format!("{}/{id}", categories(ta)),
            &oa,
            None,
        )
    };
    assert_eq!(del(&minuman).await.status, 409);
    assert_eq!(del(&kopi).await.status, 204);
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/{}", products(ta), p["id"].as_str().unwrap()),
        Some(&oa),
        None,
    )
    .await;
    assert_eq!(r.json()["data"]["category_id"], Value::Null);
    // tenant B tidak bisa menghapus kategori tenant A
    let r = json(
        &a.router,
        Method::DELETE,
        &format!("{}/{minuman}", categories(tb)),
        Some(&ob),
        None,
    )
    .await;
    assert_eq!(r.status, 404);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn concurrent_reparenting_cannot_create_cycle(pool: PgPool) {
    // A→B dan B→A bersamaan: advisory lock per tenant → paling banyak satu berhasil.
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    for _ in 0..5 {
        let mut ids = vec![];
        for n in ["A", "B"] {
            let name = format!("{n}-{}", Uuid::new_v4().simple());
            let r = json(
                &a.router,
                Method::POST,
                &categories(t),
                Some(&o),
                Some(json!({"name": name})),
            )
            .await;
            ids.push((name, r.json()["data"]["id"].as_str().unwrap().to_owned()));
        }
        let put = |me: usize, parent: usize| {
            call(
                &a,
                Method::PUT,
                format!("{}/{}", categories(t), ids[me].1),
                &o,
                Some(json!({"name": ids[me].0, "parent_id": ids[parent].1})),
            )
        };
        let (x, y) = tokio::join!(put(0, 1), put(1, 0));
        let ok = [x.status, y.status].iter().filter(|s| **s == 200).count();
        assert_eq!(ok, 1, "{} {}", x.status, y.status);
    }
    let cyc: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM product_categories c JOIN product_categories p
           ON p.tenant_id = c.tenant_id AND p.id = c.parent_id WHERE p.parent_id IS NOT NULL",
    )
    .fetch_one(&a.owner)
    .await
    .unwrap();
    assert_eq!(cyc, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn permissions_and_cross_tenant_access(pool: PgPool) {
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let viewer = member(&pool, ta, "Viewer").await;
    let support = member(&pool, ta, "Support Agent").await;
    let catalog = member(&pool, ta, "Catalog Manager").await;
    let ob = member(&pool, tb, "Owner").await;

    let r = json(
        &a.router,
        Method::POST,
        &products(ta),
        Some(&viewer),
        Some(product("X", "x")),
    )
    .await;
    assert_eq!(r.status, 403);
    let p = create(&a, ta, &catalog, product("Kopi", "k-1")).await;
    let id = p["id"].as_str().unwrap();
    assert_eq!(status(&a, ta, &catalog, id, "active").await.status, 200);
    assert_eq!(status(&a, ta, &support, id, "archived").await.status, 403);
    assert_eq!(status(&a, ta, &catalog, id, "archived").await.status, 200);
    let r = json(&a.router, Method::GET, &products(ta), Some(&viewer), None).await;
    assert_eq!(r.json()["data"]["total"], 1);
    let r = json(
        &a.router,
        Method::PUT,
        &format!("{}/{id}", products(ta)),
        Some(&support),
        Some(product("Y", "k-1")),
    )
    .await;
    assert_eq!(r.status, 403);

    // Tenant B: path tenant A → 403/404 (bukan anggota); path sendiri + id A → 404 (RLS)
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/{id}", products(ta)),
        Some(&ob),
        None,
    )
    .await;
    assert!(matches!(r.status, 403 | 404), "{}", r.status);
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}/{id}", products(tb)),
        Some(&ob),
        None,
    )
    .await;
    assert_eq!(r.status, 404);
    let r = json(
        &a.router,
        Method::PUT,
        &format!("{}/{id}", products(tb)),
        Some(&ob),
        Some(product("Curian", "z")),
    )
    .await;
    assert_eq!(r.status, 404);
    let r = status(&a, tb, &ob, id, "active").await;
    assert_eq!(r.status, 404);

    // Varian milik produk lain tidak dapat "dipindah" lewat id
    let q = create(&a, tb, &ob, product("Teh", "t-1")).await;
    let mut body = product("Teh", "t-1");
    body["variants"][0]["id"] = p["variants"][0]["id"].clone();
    let r = json(
        &a.router,
        Method::PUT,
        &format!("{}/{}", products(tb), q["id"].as_str().unwrap()),
        Some(&ob),
        Some(body),
    )
    .await;
    assert_eq!(codes(&r), vec!["variants:unknown_variant".to_owned()]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn images_must_be_tenant_owned_and_public_urls_are_signed(pool: PgPool) {
    let a = app(&pool).await;
    let (ta, tb) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let (oa, ob) = (
        member(&pool, ta, "Owner").await,
        member(&pool, tb, "Owner").await,
    );
    let img_a = upload(&a.router, ta, &oa, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let img_b = upload(&a.router, tb, &ob, png()).await.json()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut body = product("Kopi", "k-1");
    body["images"] = json!([format!("asset:{img_b}")]);
    let r = json(
        &a.router,
        Method::POST,
        &products(ta),
        Some(&oa),
        Some(body.clone()),
    )
    .await;
    assert_eq!(
        codes(&r),
        vec!["images:invalid_asset_ref".to_owned()],
        "gambar tenant B ditolak"
    );
    body["images"] = json!(["https://evil.example/x.png"]);
    let r = json(
        &a.router,
        Method::POST,
        &products(ta),
        Some(&oa),
        Some(body.clone()),
    )
    .await;
    assert_eq!(codes(&r), vec!["images[0]:invalid_asset_ref".to_owned()]);

    body["images"] = json!([format!("asset:{img_a}")]);
    let p = create(&a, ta, &oa, body).await;
    assert_eq!(p["images"][0], format!("asset:{img_a}"));
    status(&a, ta, &oa, p["id"].as_str().unwrap(), "active").await;

    let r = public(&a, "toko-a.ryasai.my.id", "/api/v1/public/products/kopi").await;
    let url = r.json()["data"]["images"][0].as_str().unwrap().to_owned();
    assert!(
        url.starts_with(&format!("/files/{img_a}?t={ta}&exp=")),
        "{url}"
    );
    let f = send(&a.router, Method::GET, &url, None, None, None, vec![]).await;
    assert_eq!(f.status, 200);

    // gambar yang dipakai produk tidak dapat dihapus
    let r = json(
        &a.router,
        Method::DELETE,
        &format!("/api/v1/tenants/{ta}/uploads/{img_a}"),
        Some(&oa),
        None,
    )
    .await;
    assert_eq!(r.status, 409);

    // Host tenant B tidak melihat produk tenant A
    let r = public(&a, "toko-b.ryasai.my.id", "/api/v1/public/products/kopi").await;
    assert_eq!(r.status, 404);
    assert_eq!(
        public(&a, "tidak-ada.ryasai.my.id", "/api/v1/public/products")
            .await
            .status,
        404
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn inactive_variants_hidden_publicly_and_search_works(pool: PgPool) {
    let a = app(&pool).await;
    let t = tenant(&pool, "toko-a").await;
    let o = member(&pool, t, "Owner").await;
    let mut body = sized("Es Teh Manis", ["et-s", "et-l"]);
    body["variants"][1]["is_active"] = json!(false);
    let p = create(&a, t, &o, body).await;
    status(&a, t, &o, p["id"].as_str().unwrap(), "active").await;
    create(&a, t, &o, product("Roti Bakar", "rb-1")).await;

    let r = public(
        &a,
        "toko-a.ryasai.my.id",
        "/api/v1/public/products/es-teh-manis",
    )
    .await;
    let vs = r.json()["data"]["variants"].as_array().unwrap().clone();
    assert_eq!(vs.len(), 1);
    assert_eq!(vs[0]["name"], "S");

    let search = |q: &str| call(&a, Method::GET, format!("{}?q={q}", products(t)), &o, None);
    assert_eq!(search("teh").await.json()["data"]["total"], 1);
    assert_eq!(search("RB-1").await.json()["data"]["total"], 1, "cari SKU");
    assert_eq!(
        search("%25").await.json()["data"]["total"],
        0,
        "% literal, bukan wildcard"
    );
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}?status=draft", products(t)),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(r.json()["data"]["total"], 1);
    let r = json(
        &a.router,
        Method::GET,
        &format!("{}?status=bogus", products(t)),
        Some(&o),
        None,
    )
    .await;
    assert_eq!(r.status, 422);
}
