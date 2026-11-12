//! Approval engine reusable (P4-2, `PRD.md §4.9`, FR-APR-001..003). Murni: tipe entity,
//! kebijakan multi-step, aturan siapa boleh memutus langkah mana.

use std::collections::BTreeSet;

use uuid::Uuid;

pub const MAX_STEPS: usize = 5;
pub const MAX_ASSIGNEES: usize = 20;
pub const TIMEOUT_MIN_HOURS: i32 = 1;
pub const TIMEOUT_MAX_HOURS: i32 = 24 * 30;
pub const REASON_MAX: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntityType {
    Order,
    Refund,
    StockAdjustment,
    PriceChange,
    AiResponse,
    BusinessConfig,
}

pub const ENTITY_TYPES: [EntityType; 6] = [
    EntityType::Order,
    EntityType::Refund,
    EntityType::StockAdjustment,
    EntityType::PriceChange,
    EntityType::AiResponse,
    EntityType::BusinessConfig,
];

impl EntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Order => "order",
            Self::Refund => "refund",
            Self::StockAdjustment => "stock_adjustment",
            Self::PriceChange => "price_change",
            Self::AiResponse => "ai_response",
            Self::BusinessConfig => "business_config",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        ENTITY_TYPES.into_iter().find(|e| e.as_str() == s)
    }
}

/// Satu langkah: siapa yang boleh memutus. Kosong keduanya = siapa pun dengan `approvals.decide`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    #[serde(default)]
    pub role_ids: Vec<Uuid>,
    #[serde(default)]
    pub user_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
}

impl RequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "expired" => Some(Self::Expired),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApprovalError {
    #[error("kebijakan tidak valid: {0}")]
    InvalidPolicy(&'static str),
    #[error("permintaan tidak lagi menunggu keputusan")]
    NotPending,
    #[error("langkah {expected} harus diputus lebih dulu")]
    WrongStep { expected: usize },
    #[error("Anda tidak ditugaskan untuk langkah ini")]
    NotAssigned,
    #[error("pemohon tidak boleh menyetujui permintaannya sendiri")]
    SelfApproval,
    #[error("penyetuju yang sama tidak boleh memutus dua langkah")]
    RepeatApprover,
    #[error("alasan penolakan wajib")]
    ReasonRequired,
}

pub fn validate_policy(steps: &[Step], timeout_hours: i32) -> Result<(), ApprovalError> {
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(ApprovalError::InvalidPolicy("1..=5 langkah"));
    }
    if steps
        .iter()
        .any(|s| s.role_ids.len() + s.user_ids.len() > MAX_ASSIGNEES)
    {
        return Err(ApprovalError::InvalidPolicy(
            "maks 20 penyetuju per langkah",
        ));
    }
    if !(TIMEOUT_MIN_HOURS..=TIMEOUT_MAX_HOURS).contains(&timeout_hours) {
        return Err(ApprovalError::InvalidPolicy("timeout 1..=720 jam"));
    }
    Ok(())
}

/// Siapa yang sedang memutus: user + role-nya + user yang mendelegasikan kepadanya (aktif).
#[derive(Debug, Clone)]
pub struct Decider {
    pub user_id: Uuid,
    pub role_id: Uuid,
    /// (user, role) pemberi delegasi yang masih berlaku
    pub delegators: Vec<(Uuid, Uuid)>,
}

#[derive(Debug, Clone)]
pub struct RequestState {
    pub status: RequestStatus,
    pub current_step: usize,
    pub steps: Vec<Step>,
    pub requested_by: Uuid,
    /// Penyetuju langkah sebelumnya (efektif, termasuk pemberi delegasi)
    pub prior_approvers: BTreeSet<Uuid>,
}

fn eligible(step: &Step, user: Uuid, role: Uuid) -> bool {
    (step.role_ids.is_empty() && step.user_ids.is_empty())
        || step.user_ids.contains(&user)
        || step.role_ids.contains(&role)
}

/// AC-APR-002/003: hanya langkah yang sedang aktif; penyetuju harus ditugaskan (langsung atau
/// lewat delegasi); bukan pemohon; tidak memutus dua langkah. Mengembalikan user "atas nama"
/// (diri sendiri atau pemberi delegasi) untuk dicatat di riwayat.
pub fn check_decision(
    r: &RequestState,
    step: usize,
    d: &Decider,
    approve: bool,
    reason: Option<&str>,
) -> Result<Uuid, ApprovalError> {
    if r.status != RequestStatus::Pending {
        return Err(ApprovalError::NotPending);
    }
    if step != r.current_step {
        return Err(ApprovalError::WrongStep {
            expected: r.current_step,
        });
    }
    if !approve && reason.is_none_or(|x| x.trim().is_empty()) {
        return Err(ApprovalError::ReasonRequired);
    }
    let s = r.steps.get(step).ok_or(ApprovalError::NotPending)?;
    let on_behalf = if eligible(s, d.user_id, d.role_id) {
        d.user_id
    } else {
        d.delegators
            .iter()
            .find(|(u, role)| eligible(s, *u, *role))
            .map(|(u, _)| *u)
            .ok_or(ApprovalError::NotAssigned)?
    };
    if approve {
        if d.user_id == r.requested_by || on_behalf == r.requested_by {
            return Err(ApprovalError::SelfApproval);
        }
        if r.prior_approvers.contains(&d.user_id) || r.prior_approvers.contains(&on_behalf) {
            return Err(ApprovalError::RepeatApprover);
        }
    }
    Ok(on_behalf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn state(steps: Vec<Step>) -> RequestState {
        RequestState {
            status: RequestStatus::Pending,
            current_step: 0,
            steps,
            requested_by: u(1),
            prior_approvers: BTreeSet::new(),
        }
    }

    fn d(user: u128, role: u128) -> Decider {
        Decider {
            user_id: u(user),
            role_id: u(role),
            delegators: vec![],
        }
    }

    #[test]
    fn sequential_steps_and_assignment() {
        let s = state(vec![
            Step {
                role_ids: vec![u(100)],
                user_ids: vec![],
            },
            Step {
                role_ids: vec![],
                user_ids: vec![u(3)],
            },
        ]);
        // AC-APR-002: langkah 2 sebelum langkah 1 ditolak
        assert_eq!(
            check_decision(&s, 1, &d(3, 9), true, None),
            Err(ApprovalError::WrongStep { expected: 0 })
        );
        assert_eq!(
            check_decision(&s, 0, &d(2, 9), true, None),
            Err(ApprovalError::NotAssigned)
        );
        assert_eq!(check_decision(&s, 0, &d(2, 100), true, None), Ok(u(2)));
        let mut s2 = s.clone();
        s2.current_step = 1;
        s2.prior_approvers.insert(u(3));
        assert_eq!(
            check_decision(&s2, 1, &d(3, 100), true, None),
            Err(ApprovalError::RepeatApprover)
        );
    }

    #[test]
    fn self_approval_reason_and_delegation() {
        let s = state(vec![Step {
            role_ids: vec![],
            user_ids: vec![u(1), u(5)],
        }]);
        assert_eq!(
            check_decision(&s, 0, &d(1, 9), true, None),
            Err(ApprovalError::SelfApproval)
        );
        assert_eq!(
            check_decision(&s, 0, &d(5, 9), false, Some(" ")),
            Err(ApprovalError::ReasonRequired)
        );
        // delegasi: user 7 bertindak atas nama user 5
        let mut del = d(7, 9);
        del.delegators.push((u(5), u(9)));
        assert_eq!(check_decision(&s, 0, &del, true, None), Ok(u(5)));
        // delegasi dari pemohon tidak membuat pemohon menyetujui dirinya
        let mut by_req = d(8, 9);
        by_req.delegators.push((u(1), u(9)));
        assert_eq!(
            check_decision(&s, 0, &by_req, true, None),
            Err(ApprovalError::SelfApproval)
        );
        let mut done = s.clone();
        done.status = RequestStatus::Expired;
        assert_eq!(
            check_decision(&done, 0, &d(5, 9), true, None),
            Err(ApprovalError::NotPending)
        );
    }

    #[test]
    fn policy_limits_and_types() {
        assert!(validate_policy(&[], 24).is_err());
        assert!(
            validate_policy(
                &[Step {
                    role_ids: vec![],
                    user_ids: vec![]
                }],
                0
            )
            .is_err()
        );
        assert!(
            validate_policy(
                &vec![
                    Step {
                        role_ids: vec![],
                        user_ids: vec![]
                    };
                    6
                ],
                24
            )
            .is_err()
        );
        assert!(
            validate_policy(
                &[Step {
                    role_ids: vec![],
                    user_ids: vec![]
                }],
                24
            )
            .is_ok()
        );
        for e in ENTITY_TYPES {
            assert_eq!(EntityType::parse(e.as_str()), Some(e));
        }
    }
}
