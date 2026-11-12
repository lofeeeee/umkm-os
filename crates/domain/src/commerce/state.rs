//! State machine order 13 status (P3-6, `PRD.md §4.7`, FR-ORD-004).
//!
//! Alur QRIS statis (ADR-0004):
//! ```text
//! checkout ─▶ AWAITING_PAYMENT ─(verifikasi staf)─▶ PAID ─┬─▶ PROCESSING ──▶ READY_FOR_PICKUP ─▶ COMPLETED
//!     │              │                                     │        │        └▶ SHIPPED ──────────▶ COMPLETED
//!     │              ├─▶ CANCELLED                         └─▶ APPROVAL_PENDING ─▶ APPROVED ─▶ PROCESSING
//!     │              └─▶ EXPIRED (reservasi habis)                     └─▶ REJECTED ─▶ REFUNDED
//!  PENDING (order dibuat staf, belum ditagih) ─▶ AWAITING_PAYMENT | CANCELLED
//! ```
//! Order yang sudah dibayar tidak "dibatalkan" begitu saja: jalurnya REFUNDED (refund dicatat).
//! Status terminal: COMPLETED tidak terminal penuh (boleh REFUNDED untuk retur);
//! CANCELLED, EXPIRED, REFUNDED terminal.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OrderStatus {
    Pending,
    AwaitingPayment,
    Paid,
    ApprovalPending,
    Approved,
    Processing,
    ReadyForPickup,
    Shipped,
    Completed,
    Rejected,
    Cancelled,
    Expired,
    Refunded,
}

pub const ALL_STATUSES: [OrderStatus; 13] = [
    OrderStatus::Pending,
    OrderStatus::AwaitingPayment,
    OrderStatus::Paid,
    OrderStatus::ApprovalPending,
    OrderStatus::Approved,
    OrderStatus::Processing,
    OrderStatus::ReadyForPickup,
    OrderStatus::Shipped,
    OrderStatus::Completed,
    OrderStatus::Rejected,
    OrderStatus::Cancelled,
    OrderStatus::Expired,
    OrderStatus::Refunded,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fulfillment {
    Pickup,
    Delivery,
}

impl Fulfillment {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pickup" => Some(Self::Pickup),
            "delivery" => Some(Self::Delivery),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pickup => "pickup",
            Self::Delivery => "delivery",
        }
    }
}

impl OrderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::AwaitingPayment => "AWAITING_PAYMENT",
            Self::Paid => "PAID",
            Self::ApprovalPending => "APPROVAL_PENDING",
            Self::Approved => "APPROVED",
            Self::Processing => "PROCESSING",
            Self::ReadyForPickup => "READY_FOR_PICKUP",
            Self::Shipped => "SHIPPED",
            Self::Completed => "COMPLETED",
            Self::Rejected => "REJECTED",
            Self::Cancelled => "CANCELLED",
            Self::Expired => "EXPIRED",
            Self::Refunded => "REFUNDED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        ALL_STATUSES.into_iter().find(|x| x.as_str() == s)
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Expired | Self::Refunded)
    }

    /// Order sudah dibayar (stok sudah dipotong; keluar hanya lewat refund).
    pub fn is_paid(self) -> bool {
        matches!(
            self,
            Self::Paid
                | Self::ApprovalPending
                | Self::Approved
                | Self::Processing
                | Self::ReadyForPickup
                | Self::Shipped
                | Self::Completed
                | Self::Rejected
        )
    }

    /// AC-ORD-004: hanya transisi yang terdaftar; fulfillment menentukan cabang kirim/ambil.
    pub fn can_transition(self, to: Self, f: Fulfillment) -> bool {
        use OrderStatus::*;
        match (self, to) {
            (Pending, AwaitingPayment | Cancelled) => true,
            (AwaitingPayment, Paid | Cancelled | Expired) => true,
            (Paid, Processing | ApprovalPending | Refunded) => true,
            (ApprovalPending, Approved | Rejected) => true,
            (Approved, Processing | Refunded) => true,
            (Processing, ReadyForPickup) => f == Fulfillment::Pickup,
            (Processing, Shipped) => f == Fulfillment::Delivery,
            (Processing, Refunded) => true,
            (ReadyForPickup, Completed | Refunded) => true,
            (Shipped, Completed | Refunded) => true,
            (Completed, Refunded) => true,
            (Rejected, Refunded) => true,
            _ => false,
        }
    }

    /// Transisi yang boleh diminta staf lewat endpoint status umum. Sisanya punya
    /// endpoint/aliran sendiri (pembayaran, approval, refund, expiry job).
    pub fn manual_target(self) -> bool {
        matches!(
            self,
            Self::Processing
                | Self::ReadyForPickup
                | Self::Shipped
                | Self::Completed
                | Self::Cancelled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use OrderStatus::*;

    #[test]
    fn exactly_the_registered_edges() {
        let mut n = 0;
        for a in ALL_STATUSES {
            for b in ALL_STATUSES {
                let p = a.can_transition(b, Fulfillment::Pickup);
                let d = a.can_transition(b, Fulfillment::Delivery);
                n += usize::from(p || d);
                assert!(!(a == b && (p || d)), "self loop {a:?}");
                if a.is_terminal() {
                    assert!(!p && !d, "terminal {a:?} -> {b:?}");
                }
            }
        }
        assert_eq!(n, 21, "jumlah edge berubah: perbarui diagram + FSD");
    }

    #[test]
    fn invalid_examples_rejected() {
        // AC-ORD-004
        let f = Fulfillment::Pickup;
        assert!(!Completed.can_transition(Pending, f));
        assert!(
            !AwaitingPayment.can_transition(Processing, f),
            "tidak boleh melompati pembayaran"
        );
        assert!(
            !Paid.can_transition(Cancelled, f),
            "dibayar → refund, bukan batal"
        );
        assert!(
            !Processing.can_transition(Shipped, f),
            "pickup tidak dikirim"
        );
        assert!(Processing.can_transition(Shipped, Fulfillment::Delivery));
        assert!(!Rejected.can_transition(Approved, f));
        assert!(!Expired.can_transition(Paid, f));
    }

    #[test]
    fn parse_roundtrip_and_paid_set() {
        for s in ALL_STATUSES {
            assert_eq!(OrderStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(OrderStatus::parse("paid"), None);
        assert!(Rejected.is_paid() && !AwaitingPayment.is_paid() && !Refunded.is_paid());
    }
}
