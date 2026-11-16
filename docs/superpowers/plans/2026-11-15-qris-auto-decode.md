# QRIS Auto-Decode Implementation Plan

> **For the implementing agent:** use the `executing-plans` skill when actually carrying out this plan. Work one checkbox at a time, run the stated command after each change, and stop on unexpected failures.
>
> **Workspace/commit consent (2026-11-15):** the user explicitly chose to work in place on `main` and explicitly authorized per-task commits while executing this plan. The implementing agent must make one concise commit per completed task (only intended files, no secrets) to support review packages. This overrides the generic “isolated worktree / no commit unless asked” defaults. Do **not** push or deploy unless separately requested.

**Goal:** remove manual static-QRIS text paste by decoding the payload from the uploaded QRIS image on the server. Existing images are backfilled lazily from `GET /commerce/settings` and `GET /gopay`; manual checkout remains available when decoding fails.

**Approved design:** `docs/superpowers/specs/2026-11-15-qris-auto-decode-design.md` (Approach A, “Simpan + peringatan”).

**Architecture:**

```text
Uploaded QRIS image (stored_objects.bytes)
        │
        ├─ PUT /commerce/settings ──► decode on changed/new image ──► commerce_settings.qris_static_payload
        │                                  │
        │                                  └─ failure: clear payload, still save settings
        │
        ├─ GET /commerce/settings ──────► lazy decode/backfill if payload missing
        ├─ GET /gopay ──────────────────► lazy decode/backfill + has_static_qris
        └─ POST /public/orders/current/gopay-qris ─► read payload only; no decode
```

**Tech stack:** Rust 2024 workspace, PostgreSQL migrations, `rqrr` 0.11.0 (`default-features = false`), existing `image` 0.25.10, `tokio::task::spawn_blocking`, `umkm_domain::commerce::qris::verify`, Axum/utoipa, Next.js static export.

**Prerequisites for integration tests:**

```bash
cd /home/ryasr/umkm-os
eval "$(scripts/dev-db.sh up)"   # exports DATABASE_URL and REDIS_URL
```

---

## Chunk 1 — Migration and schema

### Task 1: Add `commerce_settings.qris_static_payload` with legacy backfill

**Files:**
- Create: `migrations/0039_qris_payload.up.sql`
- Create: `migrations/0039_qris_payload.down.sql`
- Create: `apps/api/tests/qris_decode_http.rs`
- Modify: `apps/api/Cargo.toml` (test-only `qrcode` image feature)
- Modify: `apps/api/tests/common/mod.rs` (QR PNG helpers)

- [ ] **Step 1: Add the test-only QR encoder dependency.**

  In `apps/api/Cargo.toml` under `[dev-dependencies]`, add:

  ```toml
  qrcode = { workspace = true, features = ["image"] }
  ```

  Keep the existing `image.workspace = true` dev dependency.

- [ ] **Step 2: Add reusable QRIS fixtures to `apps/api/tests/common/mod.rs`.**

  Append these helpers. `qris_payload()` reproduces the domain fixture using the public `crc16_ccitt` function; `qr_png()` generates a real QR PNG with a quiet zone.

  ```rust
  use image::{DynamicImage, ImageFormat, Luma};
  use qrcode::QrCode;
  use std::io::Cursor;
  use umkm_domain::commerce::qris::crc16_ccitt;

  /// QRIS statis valid untuk fixture uji (format TLV + CRC tag 63).
  pub fn qris_payload() -> String {
      let fields = [
          ("00", "01"),
          ("01", "11"),
          ("26", "0014COM.EXAMPLE0208ABCDEF12"),
          ("52", "5812"),
          ("53", "360"),
          ("58", "ID"),
          ("59", "WARUNG KOPI"),
          ("60", "JAKARTA"),
          ("62", "0703A01"),
      ];
      let mut body = String::new();
      for (id, v) in fields {
          body.push_str(id);
          body.push_str(&format!("{:02}", v.len()));
          body.push_str(v);
      }
      body.push_str("6304");
      let crc = crc16_ccitt(body.as_bytes());
      body.push_str(&format!("{crc:04X}"));
      body
  }

  pub fn qr_png(content: &str) -> Vec<u8> {
      let img = QrCode::new(content.as_bytes())
          .unwrap()
          .render::<Luma<u8>>()
          .min_dimensions(400, 400)
          .build();
      let mut bytes = Vec::new();
      DynamicImage::ImageLuma8(img)
          .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
          .unwrap();
      bytes
  }

  pub fn qris_png() -> Vec<u8> {
      qr_png(&qris_payload())
  }

  pub fn tampered_qris_png() -> Vec<u8> {
      let mut b = qris_payload().into_bytes();
      let i = b.len() - 1;
      b[i] = if b[i] == b'0' { b'1' } else { b'0' };
      qr_png(&String::from_utf8(b).unwrap())
  }
  ```

- [ ] **Step 3: Write the failing migration test.**

  Create `apps/api/tests/qris_decode_http.rs` with only this test for now:

  ```rust
  mod common;

  use common::*;
  use sqlx::PgPool;
  use umkm_database::MIGRATOR;

  #[sqlx::test(migrator = "MIGRATOR")]
  async fn qris_payload_column_exists(pool: PgPool) {
      let exists: bool = sqlx::query_scalar(
          "SELECT EXISTS (
               SELECT 1 FROM information_schema.columns
               WHERE table_schema = 'public'
                 AND table_name = 'commerce_settings'
                 AND column_name = 'qris_static_payload'
           )",
      )
      .fetch_one(&pool)
      .await
      .unwrap();
      assert!(exists, "migration 0039 belum menambah qris_static_payload");
  }
  ```

- [ ] **Step 4: Run the test and confirm it fails for the missing column.**

  ```bash
  cargo test -p umkm-api --test qris_decode_http qris_payload_column_exists
  ```

  Expected: assertion failure because `qris_static_payload` does not exist yet.

- [ ] **Step 5: Create `migrations/0039_qris_payload.up.sql`.**

  ```sql
  -- P10 lanjutan: payload QRIS statis diturunkan dari gambar yang sudah diunggah.
  -- Sumber kebenaran pindah ke commerce_settings; gopay_sessions.static_qris
  -- tetap ada sebagai kolom legacy untuk kompatibilitas image lama.

  ALTER TABLE commerce_settings
      ADD COLUMN qris_static_payload text;

  ALTER TABLE commerce_settings
      ADD CONSTRAINT ck_commerce_settings_qris_payload
      CHECK (qris_static_payload IS NULL OR octet_length(qris_static_payload) BETWEEN 20 AND 1024);

  UPDATE commerce_settings cs
  SET qris_static_payload = gs.static_qris
  FROM gopay_sessions gs
  WHERE cs.tenant_id = gs.tenant_id
    AND cs.qris_static_payload IS NULL
    AND gs.static_qris IS NOT NULL;
  ```

- [ ] **Step 6: Create `migrations/0039_qris_payload.down.sql`.**

  ```sql
  ALTER TABLE commerce_settings
      DROP CONSTRAINT IF EXISTS ck_commerce_settings_qris_payload;
  ALTER TABLE commerce_settings
      DROP COLUMN IF EXISTS qris_static_payload;
  ```

- [ ] **Step 7: Rebuild migration metadata and rerun the test.**

  ```bash
  touch crates/database/build.rs
  cargo test -p umkm-api --test qris_decode_http qris_payload_column_exists
  ```

  Expected: pass. Also run `cargo test -p umkm-database` (or its migration-staleness test) to confirm `MIGRATOR` sees 0039.

---

## Chunk 2 — QRIS image decoder

### Task 2: Add `rqrr` and a tested `qris_decode` module

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/infrastructure/Cargo.toml`
- Modify: `crates/infrastructure/src/lib.rs`
- Create: `crates/infrastructure/src/qris_decode.rs`
- Modify: `deny.toml` only if `cargo deny check licenses` requires it (ISC is already allowed)

- [ ] **Step 1: Add the dependency declarations.**

  In root `Cargo.toml` under `[workspace.dependencies]`:

  ```toml
  # P10 lanjutan: baca payload QRIS dari gambar yang sudah diunggah.
  # default-features=false agar tidak menarik fitur img/rqrr; decoding memakai image workspace.
  rqrr = { version = "0.11.0", default-features = false }
  ```

  In `crates/infrastructure/Cargo.toml` under `[dependencies]`: `rqrr.workspace = true`.
  Under `[dev-dependencies]`: `qrcode = { workspace = true, features = ["image"] }`.

  Run `cargo fetch` to update `Cargo.lock`.

- [ ] **Step 2: Expose the module.**

  In `crates/infrastructure/src/lib.rs`, add `pub mod qris_decode;` in alphabetical order near `pub mod rag;`.

- [ ] **Step 3: Write failing unit tests in the new module.**

  Create `crates/infrastructure/src/qris_decode.rs` with the error enum, a stub `decode_qris_image` returning `Err(QrisDecodeError::NoQr)`, and this test module:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use image::{DynamicImage, ImageFormat, Luma};
      use qrcode::QrCode;
      use std::io::Cursor;
      use umkm_domain::commerce::qris::crc16_ccitt;

      fn payload() -> String {
          let fields = [
              ("00", "01"), ("01", "11"),
              ("26", "0014COM.EXAMPLE0208ABCDEF12"),
              ("52", "5812"), ("53", "360"), ("58", "ID"),
              ("59", "WARUNG KOPI"), ("60", "JAKARTA"), ("62", "0703A01"),
          ];
          let mut body = String::new();
          for (id, v) in fields {
              body.push_str(id);
              body.push_str(&format!("{:02}", v.len()));
              body.push_str(v);
          }
          body.push_str("6304");
          let crc = crc16_ccitt(body.as_bytes());
          body.push_str(&format!("{crc:04X}"));
          body
      }

      fn qr_png(content: &str) -> Vec<u8> {
          let img = QrCode::new(content.as_bytes())
              .unwrap()
              .render::<Luma<u8>>()
              .min_dimensions(400, 400)
              .build();
          let mut bytes = Vec::new();
          DynamicImage::ImageLuma8(img)
              .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
              .unwrap();
          bytes
      }

      #[test]
      fn decodes_valid_qris_png() {
          let expected = payload();
          assert_eq!(decode_qris_image(&qr_png(&expected)).unwrap(), expected);
      }

      #[test]
      fn rejects_non_qris_qr() {
          assert_eq!(
              decode_qris_image(&qr_png("https://example.com")).unwrap_err(),
              QrisDecodeError::InvalidPayload
          );
      }

      #[test]
      fn rejects_corrupt_image() {
          assert_eq!(
              decode_qris_image(&[1, 2, 3]).unwrap_err(),
              QrisDecodeError::Image
          );
      }

      #[test]
      fn rejects_bad_crc() {
          let mut b = payload().into_bytes();
          let i = b.len() - 1;
          b[i] = if b[i] == b'0' { b'1' } else { b'0' };
          let bad = String::from_utf8(b).unwrap();
          assert_eq!(
              decode_qris_image(&qr_png(&bad)).unwrap_err(),
              QrisDecodeError::InvalidPayload
          );
      }

      fn blank_png() -> Vec<u8> {
          let img = image::ImageBuffer::<Luma<u8>, Vec<u8>>::from_pixel(64, 64, Luma([255]));
          let mut bytes = Vec::new();
          DynamicImage::ImageLuma8(img)
              .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
              .unwrap();
          bytes
      }

      #[test]
      fn rejects_blank_image() {
          assert_eq!(
              decode_qris_image(&blank_png()).unwrap_err(),
              QrisDecodeError::NoQr
          );
      }
  }
  ```

  `blank_png()` avoids relying on `QrCode::new(b"")`, which is allowed to reject empty input.

- [ ] **Step 4: Run unit tests and confirm all four/five tests fail.**

  ```bash
  cargo test -p umkm-infrastructure --lib qris_decode
  ```

- [ ] **Step 5: Replace the stub with the real decoder.**

  ```rust
  //! Decode payload QRIS statis dari gambar yang sudah diunggah (P10 lanjutan).
  //! Murni sinkron/CPU-bound; pemanggil API menjalankannya lewat `spawn_blocking`.

  use rqrr::PreparedImage;
  use umkm_domain::commerce::qris;

  /// Batas sisi terpanjang sebelum gambar diperkecil (jaga CPU & memori).
  const MAX_SIDE: u32 = 1600;
  /// Batas keras sebelum decode: tolak gambar raksasa sebelum alokasi penuh.
  const MAX_DECODE_SIDE: u32 = 4096;
  const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;
  const MIN_PAYLOAD: usize = 20;
  const MAX_PAYLOAD: usize = 1024;

  #[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
  pub enum QrisDecodeError {
      #[error("format gambar tidak dikenal atau rusak")]
      Image,
      #[error("QR tidak ditemukan / tidak dapat dibaca")]
      NoQr,
      #[error("isi QR bukan payload QRIS statis yang valid")]
      InvalidPayload,
  }

  /// Baca QR pertama yang berisi payload QRIS statis valid.
  /// Beberapa QR dalam satu gambar diizinkan: QR non-QRIS dilewati.
  pub fn decode_qris_image(bytes: &[u8]) -> Result<String, QrisDecodeError> {
      let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
          .with_guessed_format()
          .map_err(|_| QrisDecodeError::Image)?;
      let mut limits = image::Limits::default();
      limits.max_image_width = Some(MAX_DECODE_SIDE);
      limits.max_image_height = Some(MAX_DECODE_SIDE);
      limits.max_alloc = Some(MAX_DECODE_ALLOC);
      reader.limits(limits);
      let mut img = reader.decode().map_err(|_| QrisDecodeError::Image)?;
      if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
          img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Lanczos3);
      }
      let gray = img.to_luma8();
      let (w, h) = (gray.width() as usize, gray.height() as usize);
      let pixels = gray.into_raw();
      let mut prepared = PreparedImage::prepare_from_greyscale(w, h, |x, y| pixels[y * w + x]);

      let mut saw_qr = false;
      for grid in prepared.detect_grids() {
          let Ok((_, content)) = grid.decode() else { continue };
          saw_qr = true;
          let payload = content.trim().to_owned();
          if (MIN_PAYLOAD..=MAX_PAYLOAD).contains(&payload.len())
              && qris::verify(&payload).is_ok()
          {
              return Ok(payload);
          }
      }
      Err(if saw_qr { QrisDecodeError::InvalidPayload } else { QrisDecodeError::NoQr })
  }
  ```

  > **Review amendment (2026-11-16):** add `rejects_oversized_dimensions` to the
  > test module. It builds a valid white `Luma8` PNG at
  > `MAX_DECODE_SIDE + 1` square and asserts `QrisDecodeError::Image`; RED before
  > the hard cap was `NoQr`, GREEN after is `Image`.

- [ ] **Step 6: Rerun decoder tests.**

  ```bash
  cargo test -p umkm-infrastructure --lib qris_decode
  ```

  Expected: all tests pass.

- [ ] **Step 7: Verify license policy.**

  ```bash
  cargo deny check licenses
  ```

  `rqrr` is `(MIT OR Apache-2.0) AND ISC`; `deny.toml` already allows MIT, Apache-2.0, and ISC. If cargo-deny reports a new SPDX expression problem, add the narrowest exception and record it in Task 8.

---

## Chunk 3 — Repository persistence

### Task 3: Persist `qris_static_payload` in `commerce_settings`

**Files:**
- Modify: `crates/infrastructure/src/commerce_repo.rs`

> **Review amendment (2026-11-16):** this task is repo-only. The HTTP test below
> is deferred to the start of Task 4 so every task commit stays green; Task 4
> adds the test first, then the API wiring.

- [ ] **Step 1: Add a failing HTTP test for “valid QR image saves payload”.**

  Append to `apps/api/tests/qris_decode_http.rs`:

  ```rust
  use axum::http::Method;
  use serde_json::json;

  #[sqlx::test(migrator = "MIGRATOR")]
  async fn settings_save_decodes_uploaded_qris(pool: PgPool) {
      let a = app(&pool).await;
      let t = tenant(&pool, "qris-valid").await;
      let o = member(&pool, t, "Owner").await;
      let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
          .as_str().unwrap().to_owned();
      let url = format!("/api/v1/tenants/{t}/commerce/settings");
      let r = json(&a.router, Method::PUT, &url, Some(&o), Some(json!({
          "qris": format!("asset:{id}"),
          "qris_receiver_name": "Toko A",
          "checkout_enabled": true,
          "delivery_fee": "0"
      }))).await;
      assert_eq!(r.status, 200, "{}", r.json());
      assert_eq!(r.json()["data"]["qris_static_ready"], true);
      let payload: Option<String> = sqlx::query_scalar(
          "SELECT qris_static_payload FROM commerce_settings WHERE tenant_id = $1",
      ).bind(t).fetch_one(&pool).await.unwrap();
      assert_eq!(payload.as_deref(), Some(qris_payload().as_str()));
  }
  ```

- [ ] **Step 2: Run the test and confirm it fails.**

  ```bash
  cargo test -p umkm-api --test qris_decode_http settings_save_decodes_uploaded_qris
  ```

  Expected: response lacks `qris_static_ready` / query fails because the column is not wired into code.

- [ ] **Step 3: Extend `Settings` in `crates/infrastructure/src/commerce_repo.rs`.**

  Add the field after `qris_receiver_name`:

  ```rust
  /// Payload EMVCo yang didekode dari `qris_object_id` (bukan input pengguna).
  pub qris_static_payload: Option<String>,
  ```

  Add `qris_static_payload: None` to `impl Default for Settings`.

- [ ] **Step 4: Update `settings_in` SELECT and mapping.**

  Add `qris_static_payload` to the SELECT list and:

  ```rust
  qris_static_payload: r.get("qris_static_payload"),
  ```

- [ ] **Step 5: Update `put_settings` carefully.**

  Read `before` already exists. Compute the payload used by the main upsert:

  ```rust
  let upsert_payload = if s.qris_object_id == before.qris_object_id {
      before.qris_static_payload.clone().or_else(|| s.qris_static_payload.clone())
  } else {
      None
  };
  ```

  Add `qris_static_payload` to the INSERT column list and value list, and to `ON CONFLICT DO UPDATE SET`. Bind `upsert_payload` as the new parameter. Keep all existing binds in order.

  After the upsert succeeds and before `audit_repo::record`, add a conditional set so a decoded payload can never be attached to a concurrently changed image:

  ```rust
  if let (Some(id), Some(payload)) = (s.qris_object_id, s.qris_static_payload.as_deref()) {
      sqlx::query(
          "UPDATE commerce_settings
           SET qris_static_payload = $3, updated_at = now()
           WHERE tenant_id = $1 AND qris_object_id = $2
             AND qris_static_payload IS DISTINCT FROM $3",
      )
      .bind(t.as_uuid())
      .bind(id)
      .bind(payload)
      .execute(&mut *tx)
      .await?;
  }
  ```

  Add `qris_static_ready` to the audit summary only if desired; do **not** log the payload.

- [ ] **Step 6: Add `backfill_qris_payload`.**

  Place it immediately after `settings`:

  ```rust
  /// Simpan payload hasil decode untuk gambar yang masih sama. Dipakai lazy backfill
  /// pada GET settings/status; tanpa audit karena bukan perubahan konfigurasi pengguna.
  pub async fn backfill_qris_payload(
      pool: &PgPool,
      t: TenantId,
      qris_id: Uuid,
      payload: &str,
  ) -> Result<bool, sqlx::Error> {
      if !(20..=1024).contains(&payload.len())
          || umkm_domain::commerce::qris::verify(payload).is_err()
      {
          return Ok(false);
      }
      let mut tx = tenant_tx(pool, t).await?;
      let n = sqlx::query(
          "UPDATE commerce_settings
           SET qris_static_payload = $3, updated_at = now()
           WHERE tenant_id = $1 AND qris_object_id = $2 AND qris_static_payload IS NULL",
      )
      .bind(t.as_uuid())
      .bind(qris_id)
      .bind(payload)
      .execute(&mut *tx)
      .await?
      .rows_affected();
      tx.commit().await?;
      Ok(n == 1)
  }
  ```

- [ ] **Step 7: Run repo compilation and the new HTTP test.**

  ```bash
  cargo check -p umkm-infrastructure
  cargo test -p umkm-api --test qris_decode_http settings_save_decodes_uploaded_qris
  ```

  The HTTP test will still fail until Chunk 4 adds the API response field and decode wiring; that is expected at this checkpoint. `cargo check` must pass.

---

## Chunk 4 — API settings decode, backfill, and response DTO

### Task 4: Add `SettingsView`, decode-on-save, and lazy backfill

**Files:**
- Modify: `apps/api/src/commerce.rs`
- Modify: `apps/api/tests/qris_decode_http.rs`

> **Review amendment (2026-11-16):** the HTTP test deferred from Task 3 is
> added here first, so Task 4 starts RED and ends GREEN.

- [ ] **Step 0: Add the deferred failing HTTP test.**

  Append to `apps/api/tests/qris_decode_http.rs`:

  ```rust
  use axum::http::Method;
  use serde_json::json;

  #[sqlx::test(migrator = "MIGRATOR")]
  async fn settings_save_decodes_uploaded_qris(pool: PgPool) {
      let a = app(&pool).await;
      let t = tenant(&pool, "qris-valid").await;
      let o = member(&pool, t, "Owner").await;
      let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
          .as_str().unwrap().to_owned();
      let url = format!("/api/v1/tenants/{t}/commerce/settings");
      let r = json(&a.router, Method::PUT, &url, Some(&o), Some(json!({
          "qris": format!("asset:{id}"),
          "qris_receiver_name": "Toko A",
          "checkout_enabled": true,
          "delivery_fee": "0"
      }))).await;
      assert_eq!(r.status, 200, "{}", r.json());
      assert_eq!(r.json()["data"]["qris_static_ready"], true);
      let payload: Option<String> = sqlx::query_scalar(
          "SELECT qris_static_payload FROM commerce_settings WHERE tenant_id = $1",
      ).bind(t).fetch_one(&pool).await.unwrap();
      assert_eq!(payload.as_deref(), Some(qris_payload().as_str()));
  }
  ```

  Remove the scoped `#[allow(unused_imports)]` from the test file now that the
  QR fixtures are actually used.

  ```bash
  cargo test -p umkm-api --test qris_decode_http settings_save_decodes_uploaded_qris
  ```

  Expected: RED — response lacks `qris_static_ready` and the DB payload is NULL
  until the API decode wiring is added.

- [ ] **Step 1: Add response DTO and rename conversion.**

  After `SettingsDto`, add:

  ```rust
  #[derive(Debug, Serialize, ToSchema)]
  pub struct SettingsView {
      /// `asset:<uuid>` gambar QRIS merchant milik toko
      pub qris: Option<String>,
      pub qris_receiver_name: Option<String>,
      pub checkout_enabled: bool,
      /// Ongkir datar untuk delivery, desimal string
      pub delivery_fee: String,
      pub order_approval_required: bool,
      pub reservation_ttl_minutes: i32,
      pub separation_of_duties: bool,
      /// True bila payload QRIS berhasil dibaca dari gambar.
      pub qris_static_ready: bool,
  }
  ```

  Rename `settings_dto(s: Settings) -> SettingsDto` to `settings_view(s: Settings) -> SettingsView`, returning `qris_static_ready: s.qris_static_payload.is_some()` and copying all other fields from the old function.

- [ ] **Step 2: Add imports.**

  Extend the existing `use umkm_infrastructure::{ ... }` block with:

  ```rust
  qris_decode,
  storage::ObjectStorage,
  ```

  `TenantId`, `Uuid`, `Settings`, and `internal` are already imported.

- [ ] **Step 3: Add decode helpers near the settings conversion.**

  ```rust
  async fn decode_uploaded_qris(state: &AppState, t: TenantId, id: Uuid) -> Option<String> {
      let Some(files) = state.files.as_ref() else {
          tracing::warn!(tenant = %t.as_uuid(), "object storage tidak tersedia; QRIS tidak didekode");
          return None;
      };
      let obj = match files.store.get(t, id).await {
          Ok(Some(o)) => o,
          Ok(None) => {
              tracing::warn!(tenant = %t.as_uuid(), asset = %id, "objek QRIS tidak ditemukan");
              return None;
          }
          Err(e) => {
              tracing::warn!(error = %e, "gagal membaca objek QRIS");
              return None;
          }
      };
      let bytes = obj.bytes;
      match tokio::task::spawn_blocking(move || qris_decode::decode_qris_image(&bytes)).await {
          Ok(Ok(payload)) => Some(payload),
          Ok(Err(e)) => {
              tracing::warn!(error = %e, "QRIS tidak terbaca dari gambar");
              None
          }
          Err(e) => {
              tracing::warn!(error = %e, "task decode QRIS gagal");
              None
          }
      }
  }

  /// Pastikan payload QRIS tersedia untuk gambar saat ini. Kegagalan decode tidak
  /// menggagalkan response; manual checkout tetap memakai gambar.
  pub(crate) async fn ensure_qris_payload(
      state: &AppState,
      t: TenantId,
  ) -> Result<Settings, ApiError> {
      let s = repo::settings(pool(state)?, t).await.map_err(internal)?;
      let Some(id) = s.qris_object_id else { return Ok(s) };
      if s.qris_static_payload.is_some() {
          return Ok(s);
      }
      let Some(payload) = decode_uploaded_qris(state, t, id).await else {
          return Ok(s);
      };
      let updated = repo::backfill_qris_payload(pool(state)?, t, id, &payload)
          .await
          .map_err(internal)?;
      let mut s = s;
      if updated {
          s.qris_static_payload = Some(payload);
      }
      Ok(s)
  }
  ```

- [ ] **Step 4: Add `qris_static_payload: None` to `parse_settings`.**

  This keeps `SettingsDto` unchanged and request-compatible.

- [ ] **Step 5: Resolve payload in `apply_settings`.**

  Replace the body of `apply_settings` after `actor`/`ctx` binding with this exact, compilable code. When the QRIS image is unchanged, a previously decoded payload is preserved; otherwise the new/changed image is decoded and a failure clears the payload without failing the save.

  ```rust
  let before = repo::settings(pool(state)?, actor.tenant_id)
      .await
      .map_err(internal)?;
  let mut s = parse_settings(req)?;
  s.qris_static_payload = match (s.qris_object_id, before.qris_object_id) {
      (None, _) => None,
      (Some(id), Some(prev)) if id == prev => {
          if let Some(p) = before.qris_static_payload.clone() {
              Some(p)
          } else {
              decode_uploaded_qris(state, actor.tenant_id, id).await
          }
      }
      (Some(id), _) => decode_uploaded_qris(state, actor.tenant_id, id).await,
  };
  repo::put_settings(pool(state)?, actor, &s, ctx)
      .await
      .map_err(|e| match e {
          SettingsError::QrisRequired => one("qris", "required_for_checkout"),
          SettingsError::InvalidImage => one("qris", "invalid_asset_ref"),
          SettingsError::Db(e) => internal(e),
      })?;
  Ok(s)
  ```

- [ ] **Step 6: Update GET/PUT settings handlers.**

  In `get_settings`:
  - change return type to `Result<Json<SuccessEnvelope<SettingsView>>, ApiError>`;
  - change utoipa response body to `SuccessEnvelope<SettingsView>`;
  - replace `repo::settings(...)` with `let s = ensure_qris_payload(&state, actor.tenant_id).await?;`;
  - return `settings_view(s)`.

  In `put_settings`:
  - change utoipa `200` response body to `SuccessEnvelope<SettingsView>`;
  - keep `request_body = SettingsDto`;
  - in `Gate::Direct`, replace `settings_dto(s)` with `settings_view(s)`.

- [ ] **Step 7: Switch public GoPay QRIS to the commerce payload.**

  In `current_gopay_qris`, after `get_session`, add:

  ```rust
  let settings = repo::settings(pool(&state)?, t).await.map_err(internal)?;
  ```

  Replace:

  ```rust
  let (Some(static_qris), true) = (stored.static_qris.as_deref(), stored.status == "active")
  ```

  with:

  ```rust
  let (Some(static_qris), true) = (
      settings.qris_static_payload.as_deref(),
      stored.status == "active",
  )
  ```

  Update the doc comment from “belum atur QRIS statis” to “QRIS statis belum terbaca dari gambar”. Do **not** call `ensure_qris_payload` here: public checkout must not perform image decode.

- [ ] **Step 8: Rerun the first HTTP test.**

  ```bash
  cargo test -p umkm-api --test qris_decode_http settings_save_decodes_uploaded_qris
  ```

  Expected: pass.

---

## Chunk 5 — GoPay status and endpoint removal

### Task 5: Use commerce payload for `has_static_qris` and delete manual paste API

**Files:**
- Modify: `apps/api/src/gopay.rs`
- Modify: `apps/api/src/openapi.rs`
- Modify: `docs/api/openapi.yaml` (generated)
- Modify: `apps/api/tests/qris_decode_http.rs`

- [ ] **Step 1: Add failing tests for endpoint removal and status readiness.**

  In `apps/api/tests/qris_decode_http.rs`, add:

  ```rust
  #[sqlx::test(migrator = "MIGRATOR")]
  async fn static_qris_endpoint_is_gone(pool: PgPool) {
      let a = app(&pool).await;
      let t = tenant(&pool, "qris-gone").await;
      let o = member(&pool, t, "Owner").await;
      let r = json(
          &a.router,
          Method::PUT,
          &format!("/api/v1/tenants/{t}/gopay/static-qris"),
          Some(&o),
          Some(json!({ "qris": qris_payload() })),
      ).await;
      assert_eq!(r.status, 404);
  }
  ```

  Add a connected-session helper and a status test. Import `umkm_infrastructure::gopay::GopaySession` and `gopay_repo`, `umkm_shared::ids::TenantId`, and `umkm_infrastructure::gopay::poller::now_ms`:

  ```rust
  async fn connect_gopay(a: &TestApp, t: uuid::Uuid) {
      let session = GopaySession {
          phone_e164: "+6281200000000".into(),
          merchant_id: Some("M1".into()),
          outlet_name: Some("Toko".into()),
          access_token: "access".into(),
          refresh_token: "refresh".into(),
          expires_at_ms: now_ms() + 86_400_000,
      };
      let keyring = a.state.auth.as_ref().unwrap().keyring.as_ref().unwrap();
      gopay_repo::put_session(&a.state.db.clone().unwrap(), TenantId::from_uuid(t), &session, keyring)
          .await
          .unwrap();
  }

  #[sqlx::test(migrator = "MIGRATOR")]
  async fn gopay_status_uses_decoded_qris_payload(pool: PgPool) {
      let a = app(&pool).await;
      let t = tenant(&pool, "qris-status").await;
      let o = member(&pool, t, "Owner").await;
      let id = upload(&a.router, t, &o, qris_png()).await.json()["data"]["id"]
          .as_str().unwrap().to_owned();
      let url = format!("/api/v1/tenants/{t}/commerce/settings");
      let saved = json(&a.router, Method::PUT, &url, Some(&o), Some(json!({
          "qris": format!("asset:{id}"), "qris_receiver_name": "Toko", "checkout_enabled": true
      }))).await;
      assert_eq!(saved.status, 200, "{}", saved.json());
      connect_gopay(&a, t).await;
      let status = json(
          &a.router,
          Method::GET,
          &format!("/api/v1/tenants/{t}/gopay"),
          Some(&o),
          None,
      ).await;
      assert_eq!(status.status, 200, "{}", status.json());
      assert_eq!(status.json()["data"]["connected"], true);
      assert_eq!(status.json()["data"]["has_static_qris"], true);
  }
  ```

- [ ] **Step 2: Run both tests and confirm failure.**

  ```bash
  cargo test -p umkm-api --test qris_decode_http static_qris_endpoint_is_gone
  cargo test -p umkm-api --test qris_decode_http gopay_status_uses_decoded_qris_payload
  ```

- [ ] **Step 3: Clean up `apps/api/src/gopay.rs`.**

  - Change `axum::routing::{get, post, put}` to `{get, post}`.
  - Remove `use umkm_domain::commerce::qris;`.
  - Delete `StaticQrisDto`.
  - Delete the entire `put_static_qris` handler and its `#[utoipa::path]`.
  - Delete `.route(&format!("{base}/static-qris"), put(put_static_qris))`.

- [ ] **Step 4: Compute readiness in all status responses.**

  In `get_status`, after `actor.require("tenant.read")?;`:

  ```rust
  let qris_ready = crate::commerce::ensure_qris_payload(&state, actor.tenant_id)
      .await?
      .qris_static_payload
      .is_some();
  ```

  Replace `has_static_qris: s.static_qris.is_some()` with `has_static_qris: qris_ready`; keep `has_static_qris: false` for `None` and `disconnect`.

  In `verify_otp` and `connect_session`, after `gopay_repo::put_session(...)` succeeds, compute:

  ```rust
  let qris_ready = crate::commerce::ensure_qris_payload(&state, actor.tenant_id)
      .await?
      .qris_static_payload
      .is_some();
  ```

  Replace the hard-coded `has_static_qris: false` with `has_static_qris: qris_ready`.

  Keep `gopay_repo::set_static_qris` function in the repo as legacy API; it is no longer called by HTTP.

- [ ] **Step 5: Remove the OpenAPI path entry.**

  In `apps/api/src/openapi.rs`, delete `gopay::put_static_qris,` from `paths(...)`. Do not manually list `SettingsView`; utoipa collects it from the new response body.

- [ ] **Step 6: Regenerate the OpenAPI snapshot.**

  ```bash
  UPDATE_OPENAPI=1 cargo test -p umkm-api --test openapi
  cargo test -p umkm-api --test openapi
  ```

  Confirm `docs/api/openapi.yaml` no longer contains `/gopay/static-qris` or `StaticQrisDto`, and that `SettingsView` includes `qris_static_ready`.

- [ ] **Step 7: Rerun GoPay HTTP tests.**

  ```bash
  cargo test -p umkm-api --test qris_decode_http static_qris_endpoint_is_gone
  cargo test -p umkm-api --test qris_decode_http gopay_status_uses_decoded_qris_payload
  ```

  Expected: pass.

---

## Chunk 6 — Frontend

### Task 6: Remove manual paste and show decode readiness

**Files:**
- Modify: `frontend/src/app/(app)/shop-settings/page.tsx`
- Optional: `frontend/src/shop/views.tsx` only if a copy tweak is needed; no functional change expected

- [ ] **Step 1: Update the `Settings` interface.**

  Add `qris_static_ready: boolean;` to `interface Settings`.

- [ ] **Step 2: Send only writable fields on PUT.**

  Replace `save()` with an explicit body. This is mandatory because `SettingsDto` uses `deny_unknown_fields` and would reject `qris_static_ready`.

  ```tsx
  const [refreshKey, setRefreshKey] = useState(0);
  async function save() {
    setErr(null);
    setBusy(true);
    try {
      const input = {
        qris: s!.qris,
        qris_receiver_name: s!.qris_receiver_name,
        checkout_enabled: s!.checkout_enabled,
        delivery_fee: s!.delivery_fee,
        order_approval_required: s!.order_approval_required,
        reservation_ttl_minutes: s!.reservation_ttl_minutes,
        separation_of_duties: s!.separation_of_duties,
      };
      setS(await api<Settings>("PUT", url, { json: input }));
      setRefreshKey((k) => k + 1);
      toast.ok("Tersimpan.");
    } catch (e) {
      setErr(describe(e));
    } finally {
      setBusy(false);
    }
  }
  ```

  If TypeScript narrowing complains about `s!`, capture `const current = s;` after the early return and use `current` throughout `save()`.

- [ ] **Step 3: Add the QRIS-card warning.**

  In the “Pembayaran QRIS” `<Card>`, update the description to say the system reads the QR automatically from the uploaded image. Immediately after `<ImagePicker ... />` add:

  ```tsx
  {s.qris && !s.qris_static_ready && (
    <Alert kind="warn">
      QRIS belum terbaca dari gambar. Unggah ulang gambar QRIS yang jelas; konfirmasi manual tetap tersedia.
    </Alert>
  )}
  {s.qris && s.qris_static_ready && (
    <Alert kind="ok">QRIS terbaca — konfirmasi otomatis siap.</Alert>
  )}
  ```

  Use the project’s actual `Alert` tone for success (`"ok"` or `"info"` as defined in `frontend/src/dashboard/shell.tsx`); verify before writing. Keep the existing manual-checkout explanation that the platform does not hold funds.

- [ ] **Step 4: Pass refresh signal to `GopayCard`.**

  Change:

  ```tsx
  <GopayCard tenantId={ctx.tenant.id} can={can} />
  ```

  to:

  ```tsx
  <GopayCard tenantId={ctx.tenant.id} can={can} refreshKey={refreshKey} />
  ```

  Change the signature to:

  ```tsx
  function GopayCard({ tenantId, can, refreshKey }: { tenantId: string; can: boolean; refreshKey: number }) {
  ```

  Change the status fetch effect dependency from `[base]` to `[base, refreshKey]`.

- [ ] **Step 5: Remove manual QRIS text UI from `GopayCard`.**

  Delete:
  - `const [qris, setQris] = useState("");`
  - the entire `saveQris` function;
  - the connected-branch `<label className="d-field"><span>QRIS statis toko (teks)</span> ... </label>`;
  - the “Simpan QRIS” button.

  Keep the disconnect button.

- [ ] **Step 6: Replace the connected-branch QRIS guidance.**

  Where the old textarea/hint/alert were, add:

  ```tsx
  <div className="d-field">
    <span>QRIS statis toko</span>
    <span className="d-hint">
      {st.has_static_qris
        ? "QRIS terbaca — konfirmasi otomatis siap."
        : "QRIS diambil otomatis dari gambar di kartu Pembayaran QRIS."}
    </span>
  </div>
  {!st.has_static_qris && (
    <Alert kind="warn">
      QRIS belum terbaca dari gambar. Unggah ulang/unggah QRIS yang jelas di kartu Pembayaran QRIS; konfirmasi manual tetap tersedia.
    </Alert>
  )}
  ```

- [ ] **Step 7: Verify frontend.**

  ```bash
  cd frontend
  npm run lint
  npm run typecheck
  npm run build
  npm test
  cd ..
  ```

  Ensure no reference to `/static-qris`, `saveQris`, or “QRIS statis toko (teks)” remains in `src/`.

---

## Chunk 7 — Integration coverage and regression tests

### Task 7: Cover failure, reupload, preservation, backfill, and public checkout behavior

**Files:**
- Modify: `apps/api/tests/qris_decode_http.rs`
- Modify: `apps/api/tests/checkout_http.rs` only if the `shop()` helper needs a real QR fixture for GoPay-specific tests
- Modify: `docs/api/openapi.yaml` (generated)

- [ ] **Step 1: Add “bad image still saves settings” test.**

  ```rust
  #[sqlx::test(migrator = "MIGRATOR")]
  async fn settings_save_accepts_undecodable_qris(pool: PgPool) {
      let a = app(&pool).await;
      let t = tenant(&pool, "qris-bad").await;
      let o = member(&pool, t, "Owner").await;
      let id = upload(&a.router, t, &o, png()).await.json()["data"]["id"]
          .as_str().unwrap().to_owned();
      let url = format!("/api/v1/tenants/{t}/commerce/settings");
      let r = json(&a.router, Method::PUT, &url, Some(&o), Some(json!({
          "qris": format!("asset:{id}"), "qris_receiver_name": "Toko", "checkout_enabled": true
      }))).await;
      assert_eq!(r.status, 200, "{}", r.json());
      assert_eq!(r.json()["data"]["qris_static_ready"], false);
      let payload: Option<String> = sqlx::query_scalar(
          "SELECT qris_static_payload FROM commerce_settings WHERE tenant_id = $1",
      ).bind(t).fetch_one(&pool).await.unwrap();
      assert_eq!(payload, None);
  }
  ```

- [ ] **Step 2: Add “reupload with bad image clears old payload” test.**

  Save `qris_png()`, assert `qris_static_ready == true`; upload `png()`, PUT settings with the new asset id and otherwise identical body; assert status 200, `qris_static_ready == false`, and DB payload `NULL`.

- [ ] **Step 3: Add “unchanged image preserves payload” test.**

  Save `qris_png()`; PUT the exact same settings body again; assert `qris_static_ready == true` and payload equals `qris_payload()`.

- [ ] **Step 4: Add “changed valid image replaces payload” test.**

  Use two different valid payloads by changing tag `59` (merchant name) in a helper that recomputes CRC; save image A, then image B; assert DB payload equals B. Add `fn qris_payload_named(name: &str) -> String` to `common/mod.rs` and make `qris_payload()` call it with `"WARUNG KOPI"`.

- [ ] **Step 5: Add lazy backfill test.**

  Simulate an existing installation:
  1. upload `qris_png()`;
  2. PUT settings with a **temporarily stubbed** decoder path or directly insert `commerce_settings` with `qris_static_payload = NULL` using `a.owner`;
  3. call `GET /commerce/settings`;
  4. assert `qris_static_ready == true` and DB payload equals `qris_payload()`.

  Prefer the direct SQL setup so the test does not depend on monkeypatching:

  ```rust
  sqlx::query(
      "INSERT INTO commerce_settings
       (tenant_id, qris_object_id, qris_receiver_name, checkout_enabled, qris_static_payload)
       VALUES ($1, $2, 'Toko', true, NULL)",
  )
  .bind(t)
  .bind(uuid::Uuid::parse_str(&id).unwrap())
  .execute(&pool)
  .await
  .unwrap();
  ```

- [ ] **Step 6: Add legacy migration/backfill test.**

  Insert a tenant, a valid uploaded QRIS object, a `commerce_settings` row, and a minimal `gopay_sessions` row with `static_qris = qris_payload()`; run `MIGRATOR` in a separate test only if necessary. Since `#[sqlx::test]` already applies all migrations, instead assert that migration 0039 backfilled a pre-existing row by using a dedicated migration unit test in `crates/database` if the repo has one. If no such harness exists, cover the same behavior with `backfill_qris_payload` plus an explicit SQL update from `gopay_sessions` and document that migration SQL is reviewed manually.

- [ ] **Step 7: Add GoPay status backfill test.**

  Set `commerce_settings.qris_static_payload = NULL`, connect a GoPay session, call `GET /gopay`, and assert `has_static_qris == true` plus DB payload non-null.

- [ ] **Step 8: Add public QRIS no-decode guard test.**

  Extend `checkout_http.rs` or `qris_decode_http.rs`:
  - create an order in `AWAITING_PAYMENT`;
  - set `qris_static_payload = NULL`;
  - connect active GoPay session;
  - call `POST /api/v1/public/orders/current/gopay-qris`;
  - assert `available == false`;
  - assert DB payload is still `NULL` (public endpoint did not decode).

  Reuse the existing checkout helpers from `apps/api/tests/checkout_http.rs`; if they are private, add this focused test in that file instead of duplicating the order setup.

- [ ] **Step 9: Run all new and existing HTTP tests.**

  ```bash
  cargo test -p umkm-api --test qris_decode_http
  cargo test -p umkm-api --test checkout_http
  cargo test -p umkm-api --test openapi
  ```

- [ ] **Step 10: Ensure OpenAPI snapshot matches code.**

  ```bash
  UPDATE_OPENAPI=1 cargo test -p umkm-api --test openapi
  cargo test -p umkm-api --test openapi
  ```

---

## Chunk 8 — Documentation, dependency review, and final verification

### Task 8: Record the decision and verify the whole workspace

**Files:**
- Create: `docs/decisions/0021-qris-decode-dari-gambar.md`
- Modify: `docs/architecture/dependency-review.md`
- Modify: `deny.toml` only if Task 2 required an exception
- Modify: `docs/superpowers/specs/2026-11-15-qris-auto-decode-design.md` status line if the spec has a `Draft`/`Pending` marker

- [ ] **Step 1: Update spec status.**

  If the approved spec still says `Status: Menunggu review`, change it to `Status: Disetujui (2026-11-15)` while preserving the content.

- [ ] **Step 2: Create ADR-0021.**

  Use the MADR template frontmatter:

  ```markdown
  ---
  status: Accepted
  date: 2026-11-15
  decision-makers: Product owner (pemilik repo)
  ---

  # ADR-0021 — Decode payload QRIS dari gambar yang diunggah

  ## Context and Problem Statement

  ADR-0004 memakai gambar QRIS untuk pembayaran manual. ADR-0020 menambah konfirmasi
  otomatis GoPay tetapi meminta pemilik menempel **teks** payload QRIS statis secara manual;
  ini membingungkan dan terasa seperti keharusan memasang ekstensi lagi. Padahal gambar QRIS
  sudah diunggah dan berisi payload yang sama.

  ## Considered Options

  * Mempertahankan tempel teks manual.
  * Decode di browser saat unggah.
  * Decode di server dari `stored_objects.bytes` saat settings disimpan + lazy backfill.

  ## Decision Outcome

  Chosen option: decode di server dari gambar yang sudah diunggah, karena sumber datanya sama,
  tidak menambah kerja pengguna, tidak mengekspos payload ke bundle publik, dan dapat dipakai
  ulang untuk instalasi lama. Kegagalan decode tidak menggagalkan simpan pengaturan; konfirmasi
  manual tetap tersedia.

  ### Consequences

  * Good, because pemilik tidak lagi menyalin payload QRIS atau memasang ekstensi hanya untuk QRIS.
  * Good, because `commerce_settings` menjadi sumber kebenaran payload dan status GoPay sinkron.
  * Bad, because decode bergantung pada kualitas gambar; gambar buruk menonaktifkan auto-confirm.
  * Bad, because `rqrr` menambah dependensi CPU-bound yang harus dijalankan lewat `spawn_blocking`.

  ## More Information

  ADR ini tidak menimpa ADR-0020; ia mengganti cara memperoleh payload statis. Mekanisme kode
  unik, QRIS dinamis, polling GoBiz, dan fallback manual tetap berlaku. `gopay_sessions.static_qris`
  dipertahankan sebagai kolom legacy untuk kompatibilitas rollback.
  ```

- [ ] **Step 3: Update `docs/architecture/dependency-review.md`.**

  Add a Rust table row after `qrcode`:

  ```markdown
  | rqrr | 0.11.0 | (MIT OR Apache-2.0) AND ISC | 1.85.1 | infrastructure | Decode payload QRIS dari gambar QRIS yang sudah diunggah (ADR-0021). `default-features = false` agar tidak memakai fitur `img`; gambar dibaca oleh `image` workspace, lalu `PreparedImage::prepare_from_greyscale`. Menambah `g2p` 1.x dan `lru` 0.18; lisensi keduanya dicek `cargo deny`. `qrcode` memperoleh fitur `image` **hanya di dev-dependencies** untuk fixture PNG uji | P10 lanjutan |
  ```

  Verify transitive licenses with `cargo tree -p rqrr` and `cargo deny check licenses`. If `g2p`/`lru` licenses are not already allowed, add the exact SPDX names to `deny.toml` and mention them in the row.

- [ ] **Step 4: Run backend checks.**

  ```bash
  cargo test -p umkm-infrastructure --lib
  cargo test -p umkm-infrastructure --test gopay_repo
  cargo test -p umkm-database
  cargo test -p umkm-api --test qris_decode_http
  cargo test -p umkm-api --test checkout_http
  cargo test -p umkm-api --test openapi
  cargo clippy --workspace --all-targets
  cargo deny check
  ```

- [ ] **Step 5: Run frontend checks.**

  ```bash
  cd frontend
  npm run lint
  npm run typecheck
  npm run build
  npm test
  cd ..
  ```

- [ ] **Step 6: Inspect the final diff.**

  ```bash
  git status --short
  git diff --stat
  git diff -- migrations/0039_qris_payload.up.sql apps/api/src/commerce.rs apps/api/src/gopay.rs frontend/src/app/'(app)'/shop-settings/page.tsx
  ```

  Confirm no secrets, no generated `.next/` files, no `Cargo.lock` drift beyond `rqrr`/`g2p`/`lru`, and no manual QRIS text API remains.

- [ ] **Step 7: Commit the final task, but do not push or deploy yet.**

  The user has authorized per-task commits; make the concise Task 8 commit after final verification. Report the verification results and wait for explicit user instruction to push/deploy. For deployment, rebuild `api` and `web` locally with `PUBLIC_API_ORIGIN=https://api.umkm-os.web.id` and matching `NEXT_BUILD_ID`; do not use the GHCR `web` image because it bakes the wrong API origin. Run migration 0039 before starting the new API image.

---

## Acceptance checklist

- [ ] `PUT /gopay/static-qris` returns 404 and is absent from OpenAPI.
- [ ] `GET/PUT /commerce/settings` return `qris_static_ready`; request DTO still rejects unknown fields.
- [ ] Saving a real QRIS PNG stores `commerce_settings.qris_static_payload`.
- [ ] Saving an unreadable image returns 200 with `qris_static_ready=false`; manual checkout still works.
- [ ] Replacing a readable QRIS with an unreadable image clears the old payload.
- [ ] Saving unchanged settings preserves a previously decoded payload.
- [ ] `GET /commerce/settings` and `GET /gopay` backfill missing payloads from existing images.
- [ ] `GET /gopay` reports `has_static_qris` from `commerce_settings.qris_static_payload`.
- [ ] `POST /public/orders/current/gopay-qris` only reads the payload; it never decodes.
- [ ] Frontend has no QRIS textarea, no `saveQris`, and no `/static-qris` call.
- [ ] Frontend PUT sends only writable fields and refreshes GoPay status after save.
- [ ] ADR-0021 and dependency review are updated.
- [ ] All tests, lint, typecheck, build, clippy, and cargo-deny checks pass.
