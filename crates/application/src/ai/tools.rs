//! Tool registry (P5-6, `PRD.md §4.11`, AC-AIO-002/003). LLM hanya MENGUSULKAN panggilan; backend
//! memvalidasi parameter secara ketat (field tak dikenal ditolak) dan mengotorisasi:
//! tool harus aktif untuk tenant, ditawarkan pada giliran itu, dan — untuk ScopedRead/RestrictedWrite —
//! pelanggan dalam percakapan harus terverifikasi. Identitas pelanggan TIDAK PERNAH berasal dari argumen
//! LLM (mencegah IDOR lewat prompt injection), melainkan dari konteks percakapan.

use std::future::Future;

use serde_json::{Map, Value, json};
use umkm_domain::ai::{Tool, ToolLevel};
use uuid::Uuid;

use super::llm::ToolSpec;

/// Hasil tool dibatasi agar tidak membanjiri konteks/token.
pub const RESULT_MAX_BYTES: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolArgs {
    SearchProducts { query: String, limit: u8 },
    GetProductDetail { product_id: Uuid },
    CheckStock { product_id: Uuid },
    GetBusinessHours,
    GetBusinessLocation,
    SearchFaq { query: String },
    GetCustomerOrders { limit: u8 },
    GetOrderStatus { order_number: String },
    CreateOrderDraft { items: Vec<(Uuid, u16)> },
    RequestHumanSupport { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    /// Nama tool tidak dikenal (mis. hasil injeksi "run_sql").
    #[error("tool tidak dikenal")]
    Unknown,
    /// Tool dikenal tetapi tidak diizinkan pada konteks ini.
    #[error("tool tidak diizinkan: {0}")]
    Denied(&'static str),
    #[error("parameter tidak valid: {0}")]
    Invalid(&'static str),
    #[error("eksekusi tool gagal")]
    Failed,
}

impl ToolError {
    /// Status untuk `ai_tool_calls.result_status`.
    pub fn status(&self) -> &'static str {
        match self {
            Self::Unknown | Self::Denied(_) => "denied",
            Self::Invalid(_) => "invalid",
            Self::Failed => "error",
        }
    }
}

/// Pelanggan dalam percakapan. `verified` = identitas dibuktikan (sesi akun pelanggan / token pesanan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomerCtx {
    pub customer_id: Uuid,
    pub verified: bool,
}

pub fn description(t: Tool) -> &'static str {
    match t {
        Tool::SearchProducts => "Cari produk toko berdasarkan kata kunci.",
        Tool::GetProductDetail => "Detail satu produk (harga, varian, deskripsi).",
        Tool::CheckStock => "Ketersediaan stok satu produk.",
        Tool::GetBusinessHours => "Jam operasional toko.",
        Tool::GetBusinessLocation => "Alamat dan lokasi toko.",
        Tool::SearchFaq => "Cari jawaban di FAQ / basis pengetahuan toko.",
        Tool::GetCustomerOrders => {
            "Daftar pesanan milik pelanggan yang sedang chat (terverifikasi)."
        }
        Tool::GetOrderStatus => "Status satu pesanan milik pelanggan yang sedang chat.",
        Tool::CreateOrderDraft => {
            "Buat DRAFT pesanan untuk dikonfirmasi pelanggan; tidak membayar."
        }
        Tool::RequestHumanSupport => "Alihkan percakapan ke staf manusia.",
    }
}

pub fn schema(t: Tool) -> Value {
    let obj = |props: Value, req: &[&str]| json!({ "type": "object", "properties": props, "required": req, "additionalProperties": false });
    match t {
        Tool::SearchProducts => obj(
            json!({ "query": { "type": "string", "maxLength": 100 }, "limit": { "type": "integer", "minimum": 1, "maximum": 10 } }),
            &["query"],
        ),
        Tool::GetProductDetail | Tool::CheckStock => obj(
            json!({ "product_id": { "type": "string", "format": "uuid" } }),
            &["product_id"],
        ),
        Tool::GetBusinessHours | Tool::GetBusinessLocation => obj(json!({}), &[]),
        Tool::SearchFaq => obj(
            json!({ "query": { "type": "string", "maxLength": 200 } }),
            &["query"],
        ),
        Tool::GetCustomerOrders => obj(
            json!({ "limit": { "type": "integer", "minimum": 1, "maximum": 10 } }),
            &[],
        ),
        Tool::GetOrderStatus => obj(
            json!({ "order_number": { "type": "string", "pattern": "^[A-Z0-9-]{4,32}$" } }),
            &["order_number"],
        ),
        Tool::CreateOrderDraft => obj(
            json!({ "items": { "type": "array", "minItems": 1, "maxItems": 10, "items": {
                "type": "object", "additionalProperties": false, "required": ["variant_id", "quantity"],
                "properties": { "variant_id": { "type": "string", "format": "uuid" }, "quantity": { "type": "integer", "minimum": 1, "maximum": 99 } } } } }),
            &["items"],
        ),
        Tool::RequestHumanSupport => obj(
            json!({ "reason": { "type": "string", "maxLength": 300 } }),
            &[],
        ),
    }
}

pub fn spec(t: Tool) -> ToolSpec {
    ToolSpec {
        name: t.as_str(),
        description: description(t),
        parameters: schema(t),
    }
}

fn only(m: &Map<String, Value>, allowed: &[&str]) -> Result<(), ToolError> {
    if m.keys().all(|k| allowed.contains(&k.as_str())) {
        Ok(())
    } else {
        Err(ToolError::Invalid("unknown_field"))
    }
}

fn text(
    m: &Map<String, Value>,
    k: &'static str,
    max: usize,
    required: bool,
) -> Result<String, ToolError> {
    match m.get(k) {
        None | Some(Value::Null) if !required => Ok(String::new()),
        Some(Value::String(s)) => {
            let s = s.trim();
            if (required && s.is_empty())
                || s.chars().count() > max
                || s.chars().any(char::is_control)
            {
                Err(ToolError::Invalid(k))
            } else {
                Ok(s.to_owned())
            }
        }
        _ => Err(ToolError::Invalid(k)),
    }
}

fn int(
    m: &Map<String, Value>,
    k: &'static str,
    lo: u64,
    hi: u64,
    default: Option<u64>,
) -> Result<u64, ToolError> {
    match (m.get(k), default) {
        (None | Some(Value::Null), Some(d)) => Ok(d),
        (Some(v), _) => v
            .as_u64()
            .filter(|n| (lo..=hi).contains(n))
            .ok_or(ToolError::Invalid(k)),
        _ => Err(ToolError::Invalid(k)),
    }
}

fn uuid(m: &Map<String, Value>, k: &'static str) -> Result<Uuid, ToolError> {
    m.get(k)
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(ToolError::Invalid(k))
}

/// Validasi ketat argumen (string JSON dari LLM, untrusted).
pub fn parse_args(t: Tool, raw: &str) -> Result<ToolArgs, ToolError> {
    let v: Value = if raw.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(raw).map_err(|_| ToolError::Invalid("json"))?
    };
    let m = v.as_object().ok_or(ToolError::Invalid("json"))?;
    Ok(match t {
        Tool::SearchProducts => {
            only(m, &["query", "limit"])?;
            ToolArgs::SearchProducts {
                query: text(m, "query", 100, true)?,
                limit: int(m, "limit", 1, 10, Some(5))? as u8,
            }
        }
        Tool::GetProductDetail => {
            only(m, &["product_id"])?;
            ToolArgs::GetProductDetail {
                product_id: uuid(m, "product_id")?,
            }
        }
        Tool::CheckStock => {
            only(m, &["product_id"])?;
            ToolArgs::CheckStock {
                product_id: uuid(m, "product_id")?,
            }
        }
        Tool::GetBusinessHours => {
            only(m, &[])?;
            ToolArgs::GetBusinessHours
        }
        Tool::GetBusinessLocation => {
            only(m, &[])?;
            ToolArgs::GetBusinessLocation
        }
        Tool::SearchFaq => {
            only(m, &["query"])?;
            ToolArgs::SearchFaq {
                query: text(m, "query", 200, true)?,
            }
        }
        Tool::GetCustomerOrders => {
            only(m, &["limit"])?;
            ToolArgs::GetCustomerOrders {
                limit: int(m, "limit", 1, 10, Some(5))? as u8,
            }
        }
        Tool::GetOrderStatus => {
            only(m, &["order_number"])?;
            let n = text(m, "order_number", 32, true)?;
            if n.len() < 4
                || !n
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(ToolError::Invalid("order_number"));
            }
            ToolArgs::GetOrderStatus { order_number: n }
        }
        Tool::CreateOrderDraft => {
            only(m, &["items"])?;
            let arr = m
                .get("items")
                .and_then(Value::as_array)
                .ok_or(ToolError::Invalid("items"))?;
            if arr.is_empty() || arr.len() > 10 {
                return Err(ToolError::Invalid("items"));
            }
            let mut items = Vec::with_capacity(arr.len());
            for it in arr {
                let o = it.as_object().ok_or(ToolError::Invalid("items"))?;
                only(o, &["variant_id", "quantity"])?;
                let id = uuid(o, "variant_id")?;
                if items.iter().any(|(x, _)| *x == id) {
                    return Err(ToolError::Invalid("items"));
                }
                items.push((id, int(o, "quantity", 1, 99, None)? as u16));
            }
            ToolArgs::CreateOrderDraft { items }
        }
        Tool::RequestHumanSupport => {
            only(m, &["reason"])?;
            ToolArgs::RequestHumanSupport {
                reason: text(m, "reason", 300, false)?,
            }
        }
    })
}

/// Otorisasi + validasi satu usulan panggilan tool dari LLM.
/// `offered` = tool yang ditawarkan pada giliran ini (sudah = aktif untuk tenant ∩ capability provider).
pub fn authorize(
    name: &str,
    raw_args: &str,
    offered: &[Tool],
    customer: Option<CustomerCtx>,
) -> Result<(Tool, ToolArgs), ToolError> {
    let t = Tool::parse(name).ok_or(ToolError::Unknown)?;
    if !offered.contains(&t) {
        return Err(ToolError::Denied("not_enabled"));
    }
    let args = parse_args(t, raw_args)?;
    match t.level() {
        ToolLevel::Read => {}
        ToolLevel::ScopedRead | ToolLevel::RestrictedWrite if t == Tool::RequestHumanSupport => {}
        ToolLevel::ScopedRead | ToolLevel::RestrictedWrite => {
            if !customer.is_some_and(|c| c.verified) {
                return Err(ToolError::Denied("customer_not_verified"));
            }
        }
    }
    Ok((t, args))
}

/// Eksekutor tool milik SATU tenant (dibangun dengan tenant context; tidak bisa lintas tenant).
pub trait ToolBackend: Send + Sync {
    fn execute(
        &self,
        args: &ToolArgs,
        customer: Option<CustomerCtx>,
    ) -> impl Future<Output = Result<Value, ToolError>> + Send;
}

/// Serialisasi hasil untuk dikirim balik ke LLM, dibatasi ukurannya.
pub fn result_text(v: &Value) -> String {
    let s = v.to_string();
    if s.len() <= RESULT_MAX_BYTES {
        return s;
    }
    json!({ "truncated": true, "note": "hasil terlalu besar; persempit pencarian" }).to_string()
}

/// Ringkasan argumen untuk `ai_tool_calls.args_summary` (tanpa teks bebas pelanggan).
pub fn args_summary(a: &ToolArgs) -> Value {
    match a {
        ToolArgs::SearchProducts { query, limit } => {
            json!({ "query_len": query.chars().count(), "limit": limit })
        }
        ToolArgs::GetProductDetail { product_id } | ToolArgs::CheckStock { product_id } => {
            json!({ "product_id": product_id })
        }
        ToolArgs::GetBusinessHours | ToolArgs::GetBusinessLocation => json!({}),
        ToolArgs::SearchFaq { query } => json!({ "query_len": query.chars().count() }),
        ToolArgs::GetCustomerOrders { limit } => json!({ "limit": limit }),
        ToolArgs::GetOrderStatus { order_number } => json!({ "order_number": order_number }),
        ToolArgs::CreateOrderDraft { items } => json!({ "items": items.len() }),
        ToolArgs::RequestHumanSupport { reason } => json!({ "reason_len": reason.chars().count() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umkm_domain::ai::ALL_TOOLS;

    const P: &str = "6f1c2b9e-1d2a-4c55-9a7e-1f2e3d4c5b6a";

    #[test]
    fn every_tool_has_strict_schema_and_round_trips_valid_args() {
        let valid = |t: Tool| -> String {
            match t {
                Tool::SearchProducts => r#"{"query":"kopi","limit":3}"#.into(),
                Tool::GetProductDetail | Tool::CheckStock => format!(r#"{{"product_id":"{P}"}}"#),
                Tool::SearchFaq => r#"{"query":"ongkir"}"#.into(),
                Tool::GetCustomerOrders => "{}".into(),
                Tool::GetOrderStatus => r#"{"order_number":"ORD-2026-0001"}"#.into(),
                Tool::CreateOrderDraft => {
                    format!(r#"{{"items":[{{"variant_id":"{P}","quantity":2}}]}}"#)
                }
                _ => "{}".into(),
            }
        };
        for t in ALL_TOOLS {
            assert_eq!(schema(t)["additionalProperties"], false, "{t:?}");
            assert!(parse_args(t, &valid(t)).is_ok(), "{t:?}");
            // Field tambahan selalu ditolak.
            let mut v: Value = serde_json::from_str(&valid(t)).unwrap();
            v["customer_id"] = json!(P);
            assert_eq!(
                parse_args(t, &v.to_string()),
                Err(ToolError::Invalid("unknown_field")),
                "{t:?}"
            );
        }
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        for (t, raw) in [
            (Tool::SearchProducts, r#"{"query":""}"#),
            (Tool::SearchProducts, r#"{"query":"x","limit":50}"#),
            (Tool::SearchProducts, r#"{"query":5}"#),
            (Tool::GetProductDetail, r#"{"product_id":"1 OR 1=1"}"#),
            (
                Tool::GetOrderStatus,
                r#"{"order_number":"x'; DROP TABLE orders;--"}"#,
            ),
            (Tool::CreateOrderDraft, r#"{"items":[]}"#),
            (
                Tool::CreateOrderDraft,
                &format!(r#"{{"items":[{{"variant_id":"{P}","quantity":0}}]}}"#),
            ),
            (
                Tool::CreateOrderDraft,
                &format!(r#"{{"items":[{{"variant_id":"{P}","quantity":1,"price":1}}]}}"#),
            ),
            (
                Tool::CreateOrderDraft,
                &format!(
                    r#"{{"items":[{{"variant_id":"{P}","quantity":1}},{{"variant_id":"{P}","quantity":1}}]}}"#
                ),
            ),
            (Tool::SearchFaq, "not json"),
            (Tool::SearchFaq, "[1,2]"),
        ] {
            assert!(
                matches!(parse_args(t, raw), Err(ToolError::Invalid(_))),
                "{t:?} {raw}"
            );
        }
    }

    #[test]
    fn authorization_rules() {
        let offered = [
            Tool::SearchProducts,
            Tool::GetCustomerOrders,
            Tool::RequestHumanSupport,
        ];
        let verified = Some(CustomerCtx {
            customer_id: Uuid::nil(),
            verified: true,
        });
        let guest = Some(CustomerCtx {
            customer_id: Uuid::nil(),
            verified: false,
        });
        assert_eq!(
            authorize("run_sql", "{}", &offered, verified).unwrap_err(),
            ToolError::Unknown
        );
        assert_eq!(
            authorize(
                "check_stock",
                &format!(r#"{{"product_id":"{P}"}}"#),
                &offered,
                verified
            )
            .unwrap_err(),
            ToolError::Denied("not_enabled")
        );
        assert_eq!(
            authorize("get_customer_orders", "{}", &offered, guest).unwrap_err(),
            ToolError::Denied("customer_not_verified")
        );
        assert_eq!(
            authorize("get_customer_orders", "{}", &offered, None).unwrap_err(),
            ToolError::Denied("customer_not_verified")
        );
        assert!(authorize("get_customer_orders", "{}", &offered, verified).is_ok());
        assert!(
            authorize("request_human_support", "{}", &offered, None).is_ok(),
            "handover selalu boleh bila aktif"
        );
        assert!(authorize("search_products", r#"{"query":"kopi"}"#, &offered, None).is_ok());
    }

    #[test]
    fn results_are_bounded_and_summaries_carry_no_free_text() {
        assert!(result_text(&json!({ "x": "a".repeat(10_000) })).contains("truncated"));
        let s = args_summary(&ToolArgs::SearchFaq {
            query: "alamat rumah saya jl. mawar".into(),
        })
        .to_string();
        assert!(!s.contains("mawar"));
    }
}
