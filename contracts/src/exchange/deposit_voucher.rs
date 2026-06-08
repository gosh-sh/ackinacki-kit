use std::sync::Arc;

use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::error::ExchangeModule;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;

const ABI: &str = include_str!("../../abi/exchange/DepositVoucher.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for `DepositVoucher` contract. The voucher exposes no callable
/// methods beyond `getVersion`; it is deployed by `USDCBridge` as a proof
/// that a cross-chain deposit was finalized.
pub struct DepositVoucher {
    base: ContractBase,
}

impl ModuleAccessor for DepositVoucher {
    const MODULE: KitModule = KitModule::Exchange(ExchangeModule::DepositVoucher);
}

impl HasContractBase for DepositVoucher {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for DepositVoucher {}

impl AsyncGuarded<Account> for DepositVoucher {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for DepositVoucher {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
        action(guard).await
    }
}

impl DepositVoucher {
    /// Create wrapper for a deployed `DepositVoucher` contract.
    pub fn new(
        context: Arc<ClientContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        let params = params.into();
        Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())) }
    }

    /// Wrapper bound to `address`, under the all-zero system dApp.
    pub fn new_default(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self::new(
            context,
            crate::account::ParamsOfNewContract::new(
                address.as_ref(),
                crate::dapp::SystemDapp::System,
            ),
        )
    }
}
