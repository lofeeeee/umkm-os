//! Paket & langganan (P7-1, `PRD.md §4.15`, FR-SUB-001, ADR-0016). Murni: membaca batas dari data paket
//! (`plans.limits_json`) secara fail-closed dan menghitung status efektif langganan.

use serde_json::Value;

/// Batas satu paket. `None` = tak terbatas (fair-use).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanLimits {
    pub max_members: Option<u64>,
    pub max_products: Option<u64>,
    /// Kuota token AI platform per bulan; 0 = tidak termasuk (BYOK tetap boleh bila `byok_allowed`).
    pub platform_ai_tokens_monthly: Option<u64>,
    pub byok_allowed: bool,
    pub storage_bytes: Option<u64>,
    pub conversations_monthly: Option<u64>,
    pub custom_domain: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LimitsError {
    #[error("limits_json bukan objek")]
    NotObject,
    #[error("kunci batas hilang: {0}")]
    Missing(&'static str),
    #[error("nilai batas tidak valid: {0}")]
    Invalid(&'static str),
}

pub const LIMIT_KEYS: [&str; 7] = [
    "max_members",
    "max_products",
    "platform_ai_tokens_monthly",
    "byok_allowed",
    "storage_bytes",
    "conversations_monthly",
    "custom_domain",
];

fn count(o: &serde_json::Map<String, Value>, k: &'static str) -> Result<Option<u64>, LimitsError> {
    match o.get(k) {
        None => Err(LimitsError::Missing(k)),
        Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or(LimitsError::Invalid(k)),
    }
}

fn flag(o: &serde_json::Map<String, Value>, k: &'static str) -> Result<bool, LimitsError> {
    match o.get(k) {
        None => Err(LimitsError::Missing(k)),
        Some(v) => v.as_bool().ok_or(LimitsError::Invalid(k)),
    }
}

/// Fail-closed: kunci hilang / tipe salah → error (bukan dianggap tak terbatas).
pub fn parse_limits(v: &Value) -> Result<PlanLimits, LimitsError> {
    let o = v.as_object().ok_or(LimitsError::NotObject)?;
    Ok(PlanLimits {
        max_members: count(o, "max_members")?,
        max_products: count(o, "max_products")?,
        platform_ai_tokens_monthly: count(o, "platform_ai_tokens_monthly")?,
        byok_allowed: flag(o, "byok_allowed")?,
        storage_bytes: count(o, "storage_bytes")?,
        conversations_monthly: count(o, "conversations_monthly")?,
        custom_domain: flag(o, "custom_domain")?,
    })
}

impl PlanLimits {
    /// Boleh menambah satu lagi bila pemakaian saat ini < batas.
    pub fn allows(limit: Option<u64>, current: u64) -> bool {
        limit.is_none_or(|l| current < l)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Trialing,
    Active,
    Expired,
}

impl SubscriptionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trialing => "trialing",
            Self::Active => "active",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Trialing, Self::Active, Self::Expired]
            .into_iter()
            .find(|x| x.as_str() == s)
    }
}

/// Status efektif saat `now` (detik unix): uji coba / periode yang sudah lewat = Expired, walau baris DB
/// belum diperbarui job. Website publik & checkout hanya hidup bila tidak Expired (ADR-0016).
pub fn effective(
    status: SubscriptionStatus,
    trial_ends_at: Option<i64>,
    current_period_end: Option<i64>,
    now: i64,
) -> SubscriptionStatus {
    match status {
        SubscriptionStatus::Trialing if trial_ends_at.is_some_and(|t| now < t) => {
            SubscriptionStatus::Trialing
        }
        SubscriptionStatus::Active if current_period_end.is_some_and(|t| now < t) => {
            SubscriptionStatus::Active
        }
        _ => SubscriptionStatus::Expired,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn starter() -> Value {
        json!({"max_members": 2, "max_products": 30, "platform_ai_tokens_monthly": 0, "byok_allowed": true,
               "storage_bytes": 52428800, "conversations_monthly": 100, "custom_domain": false})
    }

    #[test]
    fn limits_parse_fail_closed() {
        let l = parse_limits(&starter()).unwrap();
        assert_eq!(l.max_members, Some(2));
        assert_eq!(l.platform_ai_tokens_monthly, Some(0));
        assert!(l.byok_allowed && !l.custom_domain);
        let mut pro = starter();
        pro["conversations_monthly"] = Value::Null;
        assert_eq!(
            parse_limits(&pro).unwrap().conversations_monthly,
            None,
            "null = tak terbatas"
        );
        for k in LIMIT_KEYS {
            let mut v = starter();
            v.as_object_mut().unwrap().remove(k);
            assert_eq!(parse_limits(&v), Err(LimitsError::Missing(k)), "{k}");
        }
        let mut bad = starter();
        bad["max_products"] = json!(-1);
        assert_eq!(
            parse_limits(&bad),
            Err(LimitsError::Invalid("max_products"))
        );
        bad = starter();
        bad["byok_allowed"] = json!("ya");
        assert_eq!(
            parse_limits(&bad),
            Err(LimitsError::Invalid("byok_allowed"))
        );
        assert_eq!(parse_limits(&json!([])), Err(LimitsError::NotObject));
    }

    #[test]
    fn allows_and_effective_status() {
        assert!(PlanLimits::allows(Some(2), 1));
        assert!(!PlanLimits::allows(Some(2), 2));
        assert!(!PlanLimits::allows(Some(0), 0));
        assert!(PlanLimits::allows(None, u64::MAX - 1));
        use SubscriptionStatus::*;
        assert_eq!(effective(Trialing, Some(100), None, 99), Trialing);
        assert_eq!(effective(Trialing, Some(100), None, 100), Expired);
        assert_eq!(effective(Active, None, Some(200), 150), Active);
        assert_eq!(effective(Active, None, Some(200), 200), Expired);
        assert_eq!(effective(Active, None, None, 0), Expired);
        assert_eq!(effective(Expired, Some(1_000), Some(1_000), 0), Expired);
        for s in [Trialing, Active, Expired] {
            assert_eq!(SubscriptionStatus::parse(s.as_str()), Some(s));
        }
    }
}
