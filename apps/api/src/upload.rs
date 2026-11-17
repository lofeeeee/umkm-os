//! Upload & penyajian file (P2-6, `PRD.md §6.8`, ADR-0009).
//!
//! - `POST /api/v1/tenants/{tenant_id}/uploads`: body = byte mentah gambar. Nama file dan
//!   `Content-Type` klien diabaikan; tipe ditentukan dari isi (domain `inspect_image`).
//! - `GET /files/{object_id}?t=&exp=&sig=`: tanpa sesi; hanya signed URL valid. Semua
//!   kegagalan -> 404 seragam (tidak membocorkan apakah objek ada).

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde::{Deserialize, Serialize};
use umkm_infrastructure::storage::{ObjectStorage, StorageError, StoredObjectMeta};
use umkm_shared::http::{ErrorBody, ErrorCode, SuccessEnvelope};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, AppState, auth::ClientIp, middleware::ReqId, session::Member, state::FileService,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct UploadView {
    pub id: Uuid,
    /// Nilai untuk dipakai di `content_json` (ADR-0007), mis. `asset:<uuid>`.
    #[serde(rename = "ref")]
    pub asset_ref: String,
    pub content_type: String,
    pub byte_size: i32,
    pub created_at_unix: i64,
    /// Path signed URL relatif terhadap origin API; berlaku terbatas (ADR-0009).
    pub url: String,
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn files(state: &AppState) -> Result<&FileService, ApiError> {
    state
        .files
        .as_deref()
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceUnavailable))
}

fn view(f: &FileService, tenant: umkm_shared::ids::TenantId, m: StoredObjectMeta) -> UploadView {
    UploadView {
        url: f.signer.sign(tenant, m.id, now_unix()),
        asset_ref: format!("asset:{}", m.id),
        id: m.id,
        content_type: m.content_type,
        byte_size: m.byte_size,
        created_at_unix: m.created_at_unix,
    }
}

/// Upload ditolak (tipe/ukuran/kuota): 413 untuk ukuran, 422 lainnya.
pub fn upload_error(u: umkm_domain::storage::UploadError) -> ApiError {
    let code = if matches!(u, umkm_domain::storage::UploadError::TooLarge(_)) {
        ErrorCode::PayloadTooLarge
    } else {
        ErrorCode::ValidationFailed
    };
    ApiError(
        ErrorBody::new(code)
            .with_message(u.to_string())
            .with_details(serde_json::json!({ "reason": u.code() })),
    )
}

fn storage_error(e: StorageError) -> ApiError {
    match e {
        StorageError::Rejected(u) => upload_error(u),
        StorageError::InUse => ApiError(
            ErrorBody::new(ErrorCode::Conflict)
                .with_message("File masih dipakai di halaman website (draf atau versi terbit)"),
        ),
        StorageError::Db(e) => {
            tracing::error!(error = %e, "storage query failed");
            ApiError::new(ErrorCode::InternalError)
        }
    }
}

/// Gambar dipakai website (website.*), foto produk (products.*, P3-1), logo/QRIS (tenant.update).
fn require_any(actor: &umkm_domain::rbac::Actor, perms: &[&'static str]) -> Result<(), ApiError> {
    match perms.iter().find(|p| actor.has(p)) {
        Some(_) => Ok(()),
        None => Err(actor
            .require(perms[0])
            .err()
            .map(ApiError::from)
            .unwrap_or_else(|| ApiError::new(ErrorCode::Forbidden))),
    }
}

/// Upload gambar (AC-FILE-001/002). Butuh `website.edit`, `products.update`, atau `tenant.update`.
#[utoipa::path(
    post, path = "/api/v1/tenants/{tenant_id}/uploads", tag = "files",
    params(("tenant_id" = Uuid, Path)),
    request_body(content = Vec<u8>, content_type = "application/octet-stream",
        description = "Byte mentah PNG/JPEG/WebP, maks storage.max_object_bytes"),
    responses(
        (status = 201, description = "Tersimpan", body = SuccessEnvelope<UploadView>),
        (status = 413, description = "Terlalu besar", body = umkm_shared::http::ErrorEnvelope),
        (status = 422, description = "Tipe/struktur ditolak atau kuota habis", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn upload(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    body: Bytes,
) -> Result<(StatusCode, Json<SuccessEnvelope<UploadView>>), ApiError> {
    require_any(
        &actor,
        &["website.edit", "products.update", "tenant.update"],
    )?;
    let f = files(&state)?;
    // P7-2: kuota penyimpanan paket (total byte file tenant).
    let plan = crate::billing::plan_of(&state, actor.tenant_id).await?;
    if let Some(max) = plan.plan.limits.storage_bytes {
        let used = crate::billing::usage(&state, actor.tenant_id).await?;
        if used.storage_bytes.max(0) as u64 + body.len() as u64 > max {
            return Err(crate::billing::over_limit("storage_bytes", max));
        }
    }
    let m = f
        .store
        .put(&actor, &body, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(storage_error)?;
    tracing::info!(tenant_id = %actor.tenant_id, object_id = %m.id, bytes = m.byte_size, "object uploaded");
    Ok((
        StatusCode::CREATED,
        Json(SuccessEnvelope::new(view(f, actor.tenant_id, m), &id)),
    ))
}

/// Daftar file tenant untuk pemilih gambar editor. Butuh `website.read`.
#[utoipa::path(
    get, path = "/api/v1/tenants/{tenant_id}/uploads", tag = "files",
    params(("tenant_id" = Uuid, Path)),
    responses((status = 200, description = "File tenant (terbaru dulu)", body = SuccessEnvelope<Vec<UploadView>>))
)]
pub async fn list_uploads(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    Member(actor): Member,
) -> Result<Json<SuccessEnvelope<Vec<UploadView>>>, ApiError> {
    require_any(&actor, &["website.read", "products.read"])?;
    let f = files(&state)?;
    let items = f
        .store
        .list(actor.tenant_id)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|m| view(f, actor.tenant_id, m))
        .collect();
    Ok(Json(SuccessEnvelope::new(items, &id)))
}

#[derive(Deserialize)]
pub struct ObjectPath {
    #[allow(dead_code)]
    tenant_id: Uuid,
    object_id: Uuid,
}

/// Hapus file yang tidak dirujuk halaman mana pun. Butuh `website.edit`.
#[utoipa::path(
    delete, path = "/api/v1/tenants/{tenant_id}/uploads/{object_id}", tag = "files",
    params(("tenant_id" = Uuid, Path), ("object_id" = Uuid, Path)),
    responses(
        (status = 204, description = "Dihapus"),
        (status = 404, description = "Tidak ada", body = umkm_shared::http::ErrorEnvelope),
        (status = 409, description = "Masih dirujuk halaman", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn delete_upload(
    State(state): State<AppState>,
    ReqId(id): ReqId,
    ClientIp(ip): ClientIp,
    Member(actor): Member,
    Path(p): Path<ObjectPath>,
) -> Result<StatusCode, ApiError> {
    require_any(
        &actor,
        &["website.edit", "products.update", "tenant.update"],
    )?;
    let deleted = files(&state)?
        .store
        .delete(&actor, p.object_id, (Some(id.as_str()), Some(&ip)))
        .await
        .map_err(storage_error)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::new(ErrorCode::ResourceNotFound))
    }
}

#[derive(Deserialize)]
pub struct SignedQuery {
    t: Uuid,
    exp: u64,
    sig: String,
    /// P9-7: lebar varian untuk srcset (320/640/960/1280). Tidak ikut ditandatangani: hanya mengecilkan objek
    /// yang SAMA yang sudah lolos signature.
    w: Option<u32>,
}

/// Maks resize bersamaan per proses (container API 512 MB; decode foto besar ±50–100 MB).
static RESIZE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Header gambar yang sama untuk objek asli & varian.
fn image_response(bytes: Vec<u8>, content_type: &str, etag: &str, max_age: u64) -> Response {
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    if let Ok(ct) = HeaderValue::from_str(content_type) {
        h.insert(header::CONTENT_TYPE, ct);
    }
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    // Bila browser tetap mencoba menafsirkan sebagai dokumen: tanpa skrip, origin unik.
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-site"),
    );
    // Situs toko (subdomain) mengukur waktu muat/tampil gambar ini (LCP/Resource Timing). Tanpa
    // header ini browser menyembunyikan waktunya untuk origin lain. Objek sudah publik via URL bertanda
    // tangan dan CORP same-site membatasi penyematannya.
    h.insert("timing-allow-origin", HeaderValue::from_static("*"));
    if let Ok(cc) = HeaderValue::from_str(&format!("private, max-age={max_age}, immutable")) {
        h.insert(header::CACHE_CONTROL, cc);
    }
    if let Ok(e) = HeaderValue::from_str(&format!("\"{etag}\"")) {
        h.insert(header::ETAG, e);
    }
    res
}

/// Sajikan objek via signed URL (AC-FILE-003..005). Publik, tanpa cookie.
#[utoipa::path(
    get, path = "/files/{object_id}", tag = "files",
    params(("object_id" = Uuid, Path), ("t" = Uuid, Query), ("exp" = u64, Query), ("sig" = String, Query),
        ("w" = Option<u32>, Query, description = "Lebar varian srcset: 320 | 640 | 960 | 1280")),
    responses(
        (status = 200, description = "Byte gambar", content_type = "image/*"),
        (status = 404, description = "Signature salah/kedaluwarsa/objek tidak ada", body = umkm_shared::http::ErrorEnvelope)
    )
)]
pub async fn serve_file(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<SignedQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let not_found = || ApiError::new(ErrorCode::ResourceNotFound);
    let f = files(&state)?;
    // Path/query rusak diperlakukan sama dengan signature salah: 404 seragam.
    let (Ok(Path(object_id)), Ok(Query(q))) = (path, query) else {
        return Err(not_found());
    };
    let now = now_unix();
    let v = f
        .signer
        .verify(object_id, q.t, q.exp, &q.sig, now)
        .ok_or_else(not_found)?;
    // Tenant dari signature = tenant context RLS: objek tenant lain tidak terlihat.
    let obj = f
        .store
        .get(v.tenant, v.object)
        .await
        .map_err(storage_error)?
        .ok_or_else(not_found)?;
    let max_age = v.expires_unix.saturating_sub(now);
    let Some(w) = q.w else {
        return Ok(image_response(
            obj.bytes,
            obj.content_type.content_type(),
            &obj.sha256_hex,
            max_age,
        ));
    };
    // P9-7: varian lebar. Lebar di luar daftar → 404 seragam (bukan pintu resize sembarang ukuran).
    if !umkm_infrastructure::image_variant::WIDTHS.contains(&w) {
        return Err(not_found());
    }
    let etag = format!("{}-w{w}", obj.sha256_hex);
    if let Some((ct, bytes)) = f
        .store
        .get_variant(v.tenant, v.object, w)
        .await
        .map_err(storage_error)?
    {
        return Ok(image_response(bytes, &ct, &etag, max_age));
    }
    let ty = obj.content_type;
    // Asli sudah cukup sempit (dari header saja, tanpa decode piksel) DAN ringan → sajikan asli.
    // Asli sempit tetapi berat tetap lewat make_variant (kompres ulang; UAT 15 Nov perf #4).
    let fits = umkm_infrastructure::image_variant::display_width(&obj.bytes, ty).map(|dw| dw <= w);
    let heavy = obj.bytes.len() > umkm_infrastructure::image_variant::RECOMPRESS_MIN_BYTES;
    if fits.is_err() || (matches!(fits, Ok(true)) && !heavy) {
        return Ok(image_response(
            obj.bytes,
            ty.content_type(),
            &obj.sha256_hex,
            max_age,
        ));
    }
    let made = {
        let _permit = RESIZE
            .acquire()
            .await
            .map_err(|_| ApiError::new(ErrorCode::ServiceUnavailable))?;
        let src = obj.bytes.clone();
        tokio::task::spawn_blocking(move || {
            umkm_infrastructure::image_variant::make_variant(&src, ty, w)
        })
        .await
    };
    match made {
        Ok(Ok(Some(var))) => {
            if let Err(e) = f
                .store
                .put_variant(v.tenant, v.object, w, var.content_type, &var.bytes)
                .await
            {
                tracing::warn!(error = %e, "simpan varian gambar gagal; tetap disajikan");
            }
            Ok(image_response(var.bytes, var.content_type, &etag, max_age))
        }
        Ok(Ok(None)) => Ok(image_response(
            obj.bytes,
            ty.content_type(),
            &obj.sha256_hex,
            max_age,
        )),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "varian gambar gagal dibuat; sajikan asli");
            Ok(image_response(
                obj.bytes,
                ty.content_type(),
                &obj.sha256_hex,
                max_age,
            ))
        }
        Err(e) => {
            tracing::error!(error = %e, "tugas resize gagal");
            Ok(image_response(
                obj.bytes,
                ty.content_type(),
                &obj.sha256_hex,
                max_age,
            ))
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/tenants/{tenant_id}/uploads",
            get(list_uploads).post(upload),
        )
        .route(
            "/api/v1/tenants/{tenant_id}/uploads/{object_id}",
            delete(delete_upload),
        )
        .route("/files/{object_id}", get(serve_file))
}
