//! CRM staf (P3-3, `PRD.md §4.6`). Baca `customers.read`; ubah/hapus/export `customers.manage`.
//! Tag dan catatan internal hanya muncul di endpoint staf ini (AC-CRM-004).

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use umkm_domain::crm::{self, CustomerAddress};
use umkm_infrastructure::crm_repo::{self as repo, CrmError, Customer, CustomerSummary};
use umkm_shared::http::SuccessEnvelope;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState,
    auth::ClientIp,
    http_util::{Page, conflict, internal, not_found, pool},
    middleware::ReqId,
    session::Member,
    validation::field_errors,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerSummaryView {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub tags: Vec<String>,
    pub has_account: bool,
    pub created_at: String,
    pub deletion_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddressDto {
    pub label: Option<String>,
    pub address: String,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerView {
    #[serde(flatten)]
    pub summary: CustomerSummaryView,
    pub internal_note: Option<String>,
    pub addresses: Vec<AddressDto>,
    /// Riwayat order customer ini (AC-CRM-003); kosong bila pemanggil tanpa `orders.read`
    pub orders: Vec<crate::commerce::OrderSummaryView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CustomerList {
    pub items: Vec<CustomerSummaryView>,
    pub total: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerInput {
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub internal_note: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CustomerQuery {
    /// Cari nama/email/telepon
    pub q: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn summary_view(s: CustomerSummary) -> CustomerSummaryView {
    CustomerSummaryView {
        id: s.id,
        name: s.name,
        email: s.email,
        phone: s.phone,
        tags: s.tags,
        has_account: s.has_account,
        created_at: s.created_at,
        deletion_requested: s.deletion_requested,
    }
}

pub fn address_dto(a: CustomerAddress) -> AddressDto {
    AddressDto {
        label: a.label,
        address: a.address,
        note: a.note,
    }
}

fn view(c: Customer) -> CustomerView {
    CustomerView {
        summary: summary_view(c.summary),
        internal_note: c.internal_note,
        addresses: c.addresses.into_iter().map(address_dto).collect(),
        orders: vec![],
    }
}

fn reject(e: &[umkm_domain::business::FieldError]) -> ApiError {
    field_errors(e.iter().map(|x| (x.path.as_str(), x.code)))
}

fn repo_err(e: CrmError) -> ApiError {
    match e {
        CrmError::NotFound => not_found(),
        CrmError::Deleted => conflict("customer_deleted", "Pelanggan sedang/sudah dihapus"),
        CrmError::Db(e) => internal(e),
    }
}

fn validate(req: CustomerInput) -> Result<(crm::Contact, Vec<String>, Option<String>), ApiError> {
    let mut e = vec![];
    let c = crm::validate_contact(
        &mut e,
        &req.name,
        req.email.as_deref(),
        req.phone.as_deref(),
    );
    let tags = crm::normalize_tags(&mut e, &req.tags);
    let note = crm::validate_note(&mut e, req.internal_note.as_deref());
    match c {
        Some(c) if e.is_empty() => Ok((c, tags, note)),
        _ => Err(reject(&e)),
    }
}

type Ok<T> = Result<Json<SuccessEnvelope<T>>, ApiError>;

async fn current(
    state: &AppState,
    t: umkm_shared::ids::TenantId,
    id: Uuid,
) -> Result<CustomerView, ApiError> {
    repo::get(pool(state)?, t, id)
        .await
        .map_err(internal)?
        .filter(|c| !c.summary.anonymized)
        .map(view)
        .ok_or_else(not_found)
}

/// Daftar + pencarian customer tenant (AC-CRM-002).
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/customers", tag = "crm",
    params(("tenant_id" = Uuid, Path), CustomerQuery),
    responses((status = 200, description = "Customer", body = SuccessEnvelope<CustomerList>))
)]
pub async fn list(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Query(q): Query<CustomerQuery>,
) -> Ok<CustomerList> {
    actor.require("customers.read")?;
    let (limit, offset) = Page {
        limit: q.limit,
        offset: q.offset,
    }
    .bounds();
    let clean = |s: Option<String>| {
        s.map(|s| s.trim().chars().take(100).collect::<String>())
            .filter(|s| !s.is_empty())
    };
    let tag = clean(q.tag).map(|t| t.to_lowercase());
    let (list, total) = repo::list(
        pool(&state)?,
        actor.tenant_id,
        clean(q.q).as_deref(),
        tag.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(internal)?;
    Ok(Json(SuccessEnvelope::new(
        CustomerList {
            items: list.into_iter().map(summary_view).collect(),
            total,
        },
        &id,
    )))
}

/// Tambah customer manual (pelanggan offline).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/customers", tag = "crm",
    params(("tenant_id" = Uuid, Path)), request_body = CustomerInput,
    responses((status = 201, description = "Dibuat", body = SuccessEnvelope<CustomerView>))
)]
pub async fn create(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Json(req): Json<CustomerInput>,
) -> Result<(StatusCode, Json<SuccessEnvelope<CustomerView>>), ApiError> {
    actor.require("customers.manage")?;
    let (c, tags, note) = validate(req)?;
    let p = pool(&state)?;
    let ctx = (Some(id.as_str()), Some(ip.as_str()));
    let cid = repo::create(p, &actor, &c, ctx).await.map_err(internal)?;
    if !tags.is_empty() || note.is_some() {
        repo::update(p, &actor, cid, &c, &tags, note.as_deref(), ctx)
            .await
            .map_err(repo_err)?;
    }
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(
            current(&state, actor.tenant_id, cid).await?,
            &id,
        )),
    ))
}

/// Profil customer + alamat + tag + catatan internal.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/customers/{customer_id}", tag = "crm",
    params(("tenant_id" = Uuid, Path), ("customer_id" = Uuid, Path)),
    responses((status = 200, description = "Customer", body = SuccessEnvelope<CustomerView>))
)]
pub async fn get_one(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
    Path((_, cid)): Path<(Uuid, Uuid)>,
) -> Ok<CustomerView> {
    actor.require("customers.read")?;
    let mut v = current(&state, actor.tenant_id, cid).await?;
    if actor.has("orders.read") {
        let (list, _) = umkm_infrastructure::commerce_repo::list(
            pool(&state)?,
            actor.tenant_id,
            &umkm_infrastructure::commerce_repo::OrderFilter {
                customer_id: Some(cid),
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .map_err(internal)?;
        v.orders = list
            .into_iter()
            .map(crate::commerce::summary_view)
            .collect();
    }
    Ok(Json(SuccessEnvelope::new(v, &id)))
}

/// Ubah profil, tag, catatan internal.
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/customers/{customer_id}", tag = "crm",
    params(("tenant_id" = Uuid, Path), ("customer_id" = Uuid, Path)), request_body = CustomerInput,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<CustomerView>))
)]
pub async fn update(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, cid)): Path<(Uuid, Uuid)>,
    Json(req): Json<CustomerInput>,
) -> Ok<CustomerView> {
    actor.require("customers.manage")?;
    let (c, tags, note) = validate(req)?;
    repo::update(
        pool(&state)?,
        &actor,
        cid,
        &c,
        &tags,
        note.as_deref(),
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        current(&state, actor.tenant_id, cid).await?,
        &id,
    )))
}

/// Ganti daftar alamat (maks 5).
#[utoipa::path(
    put, path = "/api/v1/tenants/{tenant_id}/customers/{customer_id}/addresses", tag = "crm",
    params(("tenant_id" = Uuid, Path), ("customer_id" = Uuid, Path)), request_body = Vec<AddressDto>,
    responses((status = 200, description = "Tersimpan", body = SuccessEnvelope<CustomerView>))
)]
pub async fn put_addresses(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, cid)): Path<(Uuid, Uuid)>,
    Json(req): Json<Vec<AddressDto>>,
) -> Ok<CustomerView> {
    actor.require("customers.manage")?;
    let mut e = vec![];
    let list = crm::validate_addresses(
        &mut e,
        req.into_iter()
            .map(|a| CustomerAddress {
                label: a.label,
                address: a.address,
                note: a.note,
            })
            .collect(),
    );
    if !e.is_empty() {
        return Err(reject(&e));
    }
    repo::replace_addresses(
        pool(&state)?,
        &actor,
        cid,
        &list,
        (Some(id.as_str()), Some(&ip)),
    )
    .await
    .map_err(repo_err)?;
    Ok(Json(SuccessEnvelope::new(
        current(&state, actor.tenant_id, cid).await?,
        &id,
    )))
}

/// Permintaan hapus data (AC-CRM-006): anonimisasi otomatis +30 hari (ADR-0012).
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/customers/{customer_id}/deletion-request", tag = "crm",
    params(("tenant_id" = Uuid, Path), ("customer_id" = Uuid, Path)),
    responses((status = 202, description = "Dijadwalkan"))
)]
pub async fn request_deletion(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, cid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    actor.require("customers.manage")?;
    repo::request_deletion(pool(&state)?, &actor, cid, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    Ok(StatusCode::ACCEPTED)
}

/// Export data customer (JSON). Diaudit.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/customers/{customer_id}/export", tag = "crm",
    params(("tenant_id" = Uuid, Path), ("customer_id" = Uuid, Path)),
    responses((status = 200, description = "Data customer", body = SuccessEnvelope<CustomerView>))
)]
pub async fn export(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path((_, cid)): Path<(Uuid, Uuid)>,
) -> Ok<CustomerView> {
    actor.require("customers.manage")?;
    let c = repo::export(pool(&state)?, &actor, cid, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(repo_err)?;
    if c.summary.anonymized {
        return Err(not_found());
    }
    Ok(Json(SuccessEnvelope::new(view(c), &id)))
}

pub fn routes() -> Router<AppState> {
    let t = "/api/v1/tenants/{tenant_id}/customers";
    Router::new()
        .route(t, get(list).post(create))
        .route(&format!("{t}/{{customer_id}}"), get(get_one).put(update))
        .route(
            &format!("{t}/{{customer_id}}/addresses"),
            put(put_addresses),
        )
        .route(
            &format!("{t}/{{customer_id}}/deletion-request"),
            post(request_deletion),
        )
        .route(&format!("{t}/{{customer_id}}/export"), get(export))
}
