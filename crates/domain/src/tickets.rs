//! Ticketing (P4-3, `PRD.md §4.8`, FR-TKT-001..005). Murni: lifecycle 7 status, prioritas
//! + target SLA, kebijakan reopen.

pub const SUBJECT_MAX: usize = 200;
pub const BODY_MAX: usize = 5000;
pub const MAX_TAGS: usize = 10;
/// Ticket CLOSED boleh dibuka kembali dalam jendela ini; setelahnya buat ticket baru.
pub const REOPEN_WINDOW_SECS: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TicketStatus {
    Open,
    Assigned,
    InProgress,
    WaitingCustomer,
    WaitingInternal,
    Resolved,
    Closed,
}

pub const ALL_STATUSES: [TicketStatus; 7] = [
    TicketStatus::Open,
    TicketStatus::Assigned,
    TicketStatus::InProgress,
    TicketStatus::WaitingCustomer,
    TicketStatus::WaitingInternal,
    TicketStatus::Resolved,
    TicketStatus::Closed,
];

impl TicketStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Assigned => "ASSIGNED",
            Self::InProgress => "IN_PROGRESS",
            Self::WaitingCustomer => "WAITING_CUSTOMER",
            Self::WaitingInternal => "WAITING_INTERNAL",
            Self::Resolved => "RESOLVED",
            Self::Closed => "CLOSED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        ALL_STATUSES.into_iter().find(|x| x.as_str() == s)
    }

    /// AC-TKT-002: transisi eksplisit. Reopen (RESOLVED/CLOSED → IN_PROGRESS) diizinkan di sini;
    /// jendela waktu reopen untuk CLOSED dicek [`can_reopen`].
    pub fn can_transition(self, to: Self) -> bool {
        use TicketStatus::*;
        matches!(
            (self, to),
            (Open, Assigned | InProgress | Resolved | Closed)
                | (
                    Assigned,
                    Open | InProgress | WaitingCustomer | WaitingInternal | Resolved
                )
                | (InProgress, WaitingCustomer | WaitingInternal | Resolved)
                | (WaitingCustomer, InProgress | Resolved | Closed)
                | (WaitingInternal, InProgress | Resolved)
                | (Resolved, Closed | InProgress)
                | (Closed, InProgress)
        )
    }

    pub fn is_reopen(self, to: Self) -> bool {
        matches!(self, Self::Resolved | Self::Closed) && to == Self::InProgress
    }

    /// Status di mana ticket dianggap selesai (SLA resolusi terpenuhi).
    pub fn is_done(self) -> bool {
        matches!(self, Self::Resolved | Self::Closed)
    }
}

/// Reopen CLOSED hanya dalam jendela; RESOLVED selalu boleh dibuka kembali.
pub fn can_reopen(from: TicketStatus, secs_since_closed: Option<i64>) -> bool {
    match from {
        TicketStatus::Resolved => true,
        TicketStatus::Closed => secs_since_closed.is_some_and(|s| s <= REOPEN_WINDOW_SECS),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Low,
    Normal,
    High,
    Urgent,
}

impl Priority {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Self::Low),
            "normal" => Some(Self::Normal),
            "high" => Some(Self::High),
            "urgent" => Some(Self::Urgent),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }

    /// Target SLA default (detik): (respons pertama, resolusi).
    pub fn sla_secs(self) -> (i64, i64) {
        const H: i64 = 3600;
        match self {
            Self::Urgent => (H, 4 * H),
            Self::High => (4 * H, 24 * H),
            Self::Normal => (24 * H, 72 * H),
            Self::Low => (72 * H, 168 * H),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Chat,
    Order,
    Manual,
    System,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Order => "order",
            Self::Manual => "manual",
            Self::System => "system",
        }
    }
}

/// Tag: huruf kecil, 1..=30, unik; maks 10.
pub fn normalize_tags(tags: &[String]) -> Option<Vec<String>> {
    if tags.len() > MAX_TAGS {
        return None;
    }
    let mut out: Vec<String> = vec![];
    for t in tags {
        let t = t.trim().to_lowercase();
        if t.is_empty()
            || t.chars().count() > 30
            || !t
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        {
            return None;
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use TicketStatus::*;

    #[test]
    fn lifecycle() {
        // AC-TKT-002
        assert!(Open.can_transition(Assigned));
        assert!(!Open.can_transition(WaitingCustomer));
        assert!(!Closed.can_transition(Open));
        assert!(!Resolved.can_transition(Assigned));
        assert!(InProgress.can_transition(Resolved));
        for s in ALL_STATUSES {
            assert!(!s.can_transition(s), "{s:?}");
            assert_eq!(TicketStatus::parse(s.as_str()), Some(s));
        }
        assert!(Closed.is_reopen(InProgress) && !InProgress.is_reopen(Resolved));
    }

    #[test]
    fn reopen_policy() {
        // AC-TKT-005
        assert!(can_reopen(Resolved, None));
        assert!(can_reopen(Closed, Some(REOPEN_WINDOW_SECS)));
        assert!(!can_reopen(Closed, Some(REOPEN_WINDOW_SECS + 1)));
        assert!(!can_reopen(Closed, None));
    }

    #[test]
    fn priority_and_tags() {
        assert_eq!(Priority::Urgent.sla_secs(), (3600, 4 * 3600));
        assert!(Priority::parse("URGENT").is_none());
        assert_eq!(
            normalize_tags(&["Refund".into(), "refund ".into()]),
            Some(vec!["refund".to_owned()])
        );
        assert_eq!(normalize_tags(&["<x>".into()]), None);
        assert_eq!(normalize_tags(&vec!["a".to_owned(); 11]), None);
    }
}
