//! P1-5: matriks 6 role preset × 37 permission (33 P1-5 + 4 Gate D 2026-11-12) (keputusan produk 2026-11-11) dan
//! aturan membership/tenant aktif, di Postgres nyata.

use std::collections::BTreeSet;

use sqlx::PgPool;
use umkm_database::MIGRATOR;
use umkm_domain::rbac::ALL_PERMISSIONS;
use umkm_infrastructure::rbac_repo::{list_memberships, load_actor};
use umkm_shared::ids::TenantId;
use uuid::Uuid;

/// Ditulis ulang dari docs/decisions/product-decision-log.md, BUKAN disalin dari migration,
/// agar tes menangkap migration yang menyimpang dari keputusan.
fn expected(role: &str) -> BTreeSet<&'static str> {
    let all: BTreeSet<&str> = ALL_PERMISSIONS.iter().copied().collect();
    match role {
        "Owner" => all,
        "Manager" => all
            .into_iter()
            .filter(|p| {
                ![
                    "tenant.update",
                    "members.invite",
                    "members.manage",
                    "website.publish",
                    "ai_config.manage",
                    "ai_provider.manage",
                    "audit.read",
                    "payments.refund",
                ]
                .contains(p)
            })
            .collect(),
        "Support Agent" => [
            "tenant.read",
            "tickets.read",
            "tickets.reply",
            "tickets.close",
            "conversations.read",
            "conversations.reply",
            "conversations.takeover",
            "orders.read",
            "products.read",
            "customers.read",
        ]
        .into(),
        "Catalog Manager" => [
            "tenant.read",
            "products.read",
            "products.create",
            "products.update",
            "products.archive",
            "inventory.read",
            "inventory.adjust",
        ]
        .into(),
        "Viewer" => [
            "tenant.read",
            "website.read",
            "products.read",
            "inventory.read",
            "orders.read",
            "tickets.read",
            "analytics.read",
            "customers.read",
            // P6-3: percakapan dengan masking (AC-CNV-003)
            "conversations.read",
        ]
        .into(),
        "Admin" => ["tenant.read"].into(),
        other => panic!("role tak dikenal {other}"),
    }
}

const ROLES: [&str; 6] = [
    "Owner",
    "Manager",
    "Admin",
    "Support Agent",
    "Catalog Manager",
    "Viewer",
];

async fn tenant(pool: &PgPool, slug: &str) -> TenantId {
    TenantId::from_uuid(
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('T', $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .unwrap(),
    )
}

async fn user(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, password_hash) VALUES ($1, '$argon2id$x') RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn member(pool: &PgPool, t: TenantId, u: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) SELECT $1, $2, id FROM roles WHERE tenant_id IS NULL AND name = $3")
        .bind(t.as_uuid())
        .bind(u)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn six_roles_by_37_permissions_matrix(pool: PgPool) {
    let t = tenant(&pool, "toko-a").await;
    let mut checked = 0;
    for role in ROLES {
        let u = user(
            &pool,
            &format!("{}@x.id", role.replace(' ', "-").to_lowercase()),
        )
        .await;
        member(&pool, t, u, role).await;
        let actor = load_actor(&pool, u, t).await.unwrap().unwrap();
        assert_eq!(actor.role_name, role);
        let want = expected(role);
        for p in ALL_PERMISSIONS {
            assert_eq!(actor.has(p), want.contains(p), "{role} × {p}");
            assert_eq!(actor.require(p).is_ok(), want.contains(p));
            checked += 1;
        }
    }
    assert_eq!(checked, 6 * 37);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn inactive_membership_or_tenant_and_foreign_tenant_yield_no_actor(pool: PgPool) {
    let (a, b) = (tenant(&pool, "toko-a").await, tenant(&pool, "toko-b").await);
    let u = user(&pool, "u@x.id").await;
    member(&pool, a, u, "Owner").await;
    assert!(load_actor(&pool, u, a).await.unwrap().is_some());
    assert!(
        load_actor(&pool, u, b).await.unwrap().is_none(),
        "bukan anggota tenant B"
    );

    sqlx::query("UPDATE tenant_memberships SET status = 'suspended' WHERE user_id = $1")
        .bind(u)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        load_actor(&pool, u, a).await.unwrap().is_none(),
        "membership suspended"
    );
    sqlx::query("UPDATE tenant_memberships SET status = 'active' WHERE user_id = $1")
        .bind(u)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE id = $1")
        .bind(a.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        load_actor(&pool, u, a).await.unwrap().is_none(),
        "tenant suspended (BR-011)"
    );
    assert!(list_memberships(&pool, u).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn custom_role_grants_exactly_its_permissions(pool: PgPool) {
    let t = tenant(&pool, "toko-a").await;
    let u = user(&pool, "kasir@x.id").await;
    let role: Uuid =
        sqlx::query_scalar("INSERT INTO roles (tenant_id, name) VALUES ($1, 'Kasir') RETURNING id")
            .bind(t.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO role_permissions (role_id, permission_id) SELECT $1, id FROM permissions WHERE permission_key IN ('orders.read', 'orders.create')")
        .bind(role)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) VALUES ($1, $2, $3)")
        .bind(t.as_uuid())
        .bind(u)
        .bind(role)
        .execute(&pool)
        .await
        .unwrap();
    let actor = load_actor(&pool, u, t).await.unwrap().unwrap();
    let got: BTreeSet<&str> = actor.permissions().collect();
    assert_eq!(got, ["orders.create", "orders.read"].into());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn role_without_permissions_is_denied_everything(pool: PgPool) {
    let t = tenant(&pool, "toko-a").await;
    let u = user(&pool, "x@x.id").await;
    let role: Uuid = sqlx::query_scalar(
        "INSERT INTO roles (tenant_id, name) VALUES ($1, 'Kosong') RETURNING id",
    )
    .bind(t.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role_id) VALUES ($1, $2, $3)")
        .bind(t.as_uuid())
        .bind(u)
        .bind(role)
        .execute(&pool)
        .await
        .unwrap();
    let actor = load_actor(&pool, u, t).await.unwrap().unwrap();
    assert!(ALL_PERMISSIONS.iter().all(|p| actor.require(p).is_err()));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn list_memberships_only_active_for_user(pool: PgPool) {
    let (a, b, c) = (
        tenant(&pool, "toko-a").await,
        tenant(&pool, "toko-b").await,
        tenant(&pool, "toko-c").await,
    );
    let (u, v) = (user(&pool, "u@x.id").await, user(&pool, "v@x.id").await);
    member(&pool, a, u, "Owner").await;
    member(&pool, b, u, "Viewer").await;
    member(&pool, c, v, "Owner").await;
    let mut slugs: Vec<String> = list_memberships(&pool, u)
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.slug)
        .collect();
    slugs.sort();
    assert_eq!(
        slugs,
        ["toko-a", "toko-b"],
        "AUTH-008: beberapa tenant; tidak termasuk tenant v"
    );
}
