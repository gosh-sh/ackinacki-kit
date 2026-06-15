//! dApp ID helpers for talking to both GraphQL server generations.
//!
//! Acki Nacki's GraphQL server gained a breaking change at `gql-server 1.0.0`:
//! `blockchain.account(...)` dropped the single `address` argument in favour of
//! separate `account_id` + `dapp_id` arguments. The kit must keep working
//! against both generations, so address-bearing queries are gated at runtime on
//! [`supports_dapp_id`].

use std::sync::Arc;

use tvm_client::ClientContext;

use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::KitResult;

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

/// Whether the connected GraphQL server speaks the v3 dApp ID API
/// (`info.version >= "1.0.0"`).
///
/// The SDK resolves the endpoint on the first call and caches the parsed server
/// version on it, so repeated calls are cheap — hoist the result out of paging
/// loops rather than calling it per page.
pub async fn supports_dapp_id(context: &Arc<ClientContext>, module: KitModule) -> KitResult<bool> {
    context.supports_dapp_id().await.map_err(|e| {
        KitError::new(module, KitErrorCode::QueryEvents, "Detect GraphQL server version")
            .with_tvm_error(e)
    })
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
