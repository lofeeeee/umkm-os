//! Tim & permission (P4-1, `PRD.md §3`, `PRD.md §4.9`). Murni: aturan anti-eskalasi hak,
//! validasi role kustom, aturan owner terakhir.

use std::collections::BTreeSet;

use crate::rbac::ALL_PERMISSIONS;

pub const ROLE_NAME_MAX: usize = 60;
/// Undangan berlaku 7 hari (AUTH-010).
pub const INVITATION_TTL_SECS: i64 = 7 * 24 * 3600;
pub const OWNER: &str = "Owner";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeamError {
    #[error("izin tidak dikenal: {0}")]
    UnknownPermission(String),
    #[error("tidak boleh memberi izin yang tidak Anda miliki")]
    Escalation,
    #[error("tidak dapat mengubah keanggotaan sendiri")]
    SelfChange,
    #[error("toko harus memiliki minimal satu Pemilik aktif")]
    LastOwner,
    #[error("nama peran tidak valid")]
    InvalidName,
}

/// Permission kustom harus dikenal dan unik (deny-by-default untuk yang lain).
pub fn parse_permissions(keys: &[String]) -> Result<BTreeSet<&'static str>, TeamError> {
    keys.iter()
        .map(|k| {
            ALL_PERMISSIONS
                .iter()
                .copied()
                .find(|p| *p == k.as_str())
                .ok_or_else(|| TeamError::UnknownPermission(k.clone()))
        })
        .collect()
}

/// Anti-eskalasi: pelaku hanya boleh memberi/menyentuh set permission yang seluruhnya ia miliki.
pub fn check_grant<'a>(
    actor: &BTreeSet<&'a str>,
    target: impl IntoIterator<Item = &'a str>,
) -> Result<(), TeamError> {
    if target.into_iter().all(|p| actor.contains(p)) {
        Ok(())
    } else {
        Err(TeamError::Escalation)
    }
}

pub fn validate_role_name(name: &str) -> Result<String, TeamError> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > ROLE_NAME_MAX || n.chars().any(char::is_control) {
        return Err(TeamError::InvalidName);
    }
    Ok(n.to_owned())
}

/// Perubahan anggota yang akan membuat jumlah Owner aktif = 0 ditolak.
pub fn check_owner_remains(
    active_owners: usize,
    target_is_owner: bool,
    stays_active_owner: bool,
) -> Result<(), TeamError> {
    if target_is_owner && !stays_active_owner && active_owners <= 1 {
        Err(TeamError::LastOwner)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalation_guard() {
        let actor: BTreeSet<&str> = ["members.manage", "orders.read"].into();
        assert!(check_grant(&actor, ["orders.read"]).is_ok());
        assert!(check_grant(&actor, []).is_ok());
        assert_eq!(
            check_grant(&actor, ["orders.read", "audit.read"]),
            Err(TeamError::Escalation)
        );
    }

    #[test]
    fn permissions_and_names() {
        assert_eq!(
            parse_permissions(&["orders.read".into(), "orders.read".into()])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            parse_permissions(&["root.all".into()]),
            Err(TeamError::UnknownPermission("root.all".into()))
        );
        assert_eq!(validate_role_name("  Kasir  ").unwrap(), "Kasir");
        assert!(validate_role_name(" ").is_err() && validate_role_name("a\u{7}").is_err());
    }

    #[test]
    fn last_owner() {
        assert_eq!(
            check_owner_remains(1, true, false),
            Err(TeamError::LastOwner)
        );
        assert!(check_owner_remains(2, true, false).is_ok());
        assert!(check_owner_remains(1, true, true).is_ok());
        assert!(check_owner_remains(1, false, false).is_ok());
    }
}
