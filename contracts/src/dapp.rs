//! Well-known system dApp IDs.
//!
//! Acki Nacki's `gql-server >= 1.0.0` addresses accounts by `account_id` +
//! `dapp_id`. [`SystemDapp`] maps each subsystem to its fixed dApp ID.

/// Well-known system dApps in Acki Nacki.
///
/// Each maps to a fixed dApp ID (bare 64-hex, no `0x`, no workchain) whose
/// numeric value identifies the subsystem. Pass a variant straight to
/// [`ParamsOfNewContract::new`](crate::account::ParamsOfNewContract::new) when
/// constructing a system contract against a `>= 1.0.0` server:
///
/// ```ignore
/// let params = ParamsOfNewContract::new(address, SystemDapp::AuthService);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemDapp {
    /// The all-zero system dApp — hosts Block Keeper / Block Manager and the giver.
    System,
    /// Mobile Verifiers subsystem.
    MobileVerifiers,
    /// AuthService subsystem.
    AuthService,
}

impl SystemDapp {
    /// The subsystem's dApp ID as a bare 64-hex string.
    pub const fn dapp_id(self) -> &'static str {
        match self {
            SystemDapp::System => {
                "0000000000000000000000000000000000000000000000000000000000000000"
            }
            SystemDapp::MobileVerifiers => {
                "0000000000000000000000000000000000000000000000000000000000000001"
            }
            SystemDapp::AuthService => {
                "0000000000000000000000000000000000000000000000000000000000000002"
            }
        }
    }
}

impl From<SystemDapp> for String {
    fn from(dapp: SystemDapp) -> Self {
        dapp.dapp_id().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::SystemDapp;

    fn assert_dapp_id(dapp: SystemDapp, ends_with: char) {
        let id = dapp.dapp_id();
        assert_eq!(id.len(), 64, "dapp_id must be 64 hex chars");
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()), "dapp_id must be hex");
        assert_eq!(id.chars().last(), Some(ends_with));
        assert!(id[..63].bytes().all(|b| b == b'0'), "only the last nibble is significant");
    }

    #[test]
    fn system_dapp_ids_are_64_hex() {
        assert_dapp_id(SystemDapp::System, '0');
        assert_dapp_id(SystemDapp::MobileVerifiers, '1');
        assert_dapp_id(SystemDapp::AuthService, '2');
    }
}
