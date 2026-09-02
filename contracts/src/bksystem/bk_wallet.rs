use std::collections::HashMap;

use serde::Deserialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;

use crate::account::Account;
use crate::bksystem::LicenseData;
use crate::bksystem::Stake;
use crate::delivery::ContractContext;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u8;
use crate::error::BkSystemModule;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/bksystem/AckiNackiBlockKeeperNodeWallet.abi.json");

#[derive(Debug, Clone)]
pub struct BlockKeeperWallet {
    base: ContractBase,
}

impl ModuleAccessor for BlockKeeperWallet {
    const MODULE: KitModule = KitModule::BkSystem(BkSystemModule::BlockKeeperWallet);
}

impl HasContractBase for BlockKeeperWallet {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for BlockKeeperWallet {}

impl AsyncGuarded<Account> for BlockKeeperWallet {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for BlockKeeperWallet {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetDetails {
    pub pubkey: String,
    #[serde(rename = "signerPubkey")]
    pub signer_pubkey: String,
    pub root: String,
    pub balance: String,
    #[serde(rename = "activeStakes")]
    pub active_stakes: HashMap<String, Stake>,
    #[serde(rename = "stakesCnt", deserialize_with = "deserialize_u8")]
    pub stakes_cnt: u8,
    pub licenses: HashMap<String, LicenseData>,
    #[serde(rename = "epochDuration", deserialize_with = "deserialize_u128")]
    pub epoch_duration: u128,
    #[serde(rename = "whiteListLicense")]
    pub whitelist_license: HashMap<String, bool>,
}

impl BlockKeeperWallet {
    /// General constructor — caller supplies address + dApp ID.
    pub fn new(
        context: impl Into<ContractContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())) }
    }

    /// Wrapper bound to `address`, under the all-zero system dApp.
    pub fn new_default(context: impl Into<ContractContext>, address: impl AsRef<str>) -> Self {
        Self::new(
            context,
            crate::account::ParamsOfNewContract::new(
                address.as_ref(),
                crate::dapp::SystemDapp::System,
            ),
        )
    }

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }
}

#[cfg(test)]
mod tests {
    use crate::bksystem::bk_wallet::BlockKeeperWallet;
    use crate::tests::create_context;

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn test_get_details() {
        let context = create_context();

        let bk_wallet = BlockKeeperWallet::new(
            context,
            crate::account::ParamsOfNewContract::new(
                "0:733e033541ad17c4251cdf97378045e44d8eb89ddfe4659cf5b45e4376a3a02e",
                crate::dapp::SystemDapp::System,
            ),
        );

        let details = bk_wallet
            .get_details()
            .await
            .inspect_err(|e| eprintln!("Get BK wallet details ({e:?})"));
        assert!(details.is_ok());
    }
}
