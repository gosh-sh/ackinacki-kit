use std::sync::Arc;

use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::error::DexModule;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;

const ABI: &str = include_str!("../../abi/dex/Nullifier.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for the DEX `Nullifier` contract.
///
/// In practice this contract is deployed and controlled by `RootPN`; the only
/// public method exposed by ABI is `getVersion` (available via `VersionAccessor`).
pub struct Nullifier {
    base: ContractBase,
}

impl ModuleAccessor for Nullifier {
    const MODULE: KitModule = KitModule::Dex(DexModule::Nullifier);
}

impl HasContractBase for Nullifier {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for Nullifier {}

impl AsyncGuarded<Account> for Nullifier {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Nullifier {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
        action(guard).await
    }
}

impl Nullifier {
    /// Create a wrapper for a deployed `Nullifier`.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }
}
