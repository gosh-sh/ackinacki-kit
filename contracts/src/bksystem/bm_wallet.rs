use std::sync::Arc;

use serde::Deserialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u128;
use crate::error::BkSystemModule;
use crate::error::KitModule;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::GetMethodAccessor;
use crate::traits::ModuleAccessor;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/bksystem/AckiNackiBlockManagerNodeWallet.abi.json");

#[derive(Debug, Clone)]
pub struct BlockManagerWallet {
    context: Arc<ClientContext>,
    address: String,
    dapp_id: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for BlockManagerWallet {
    const MODULE: KitModule = KitModule::BkSystem(BkSystemModule::BlockKeeperWallet);
}

impl AccountAccessor for BlockManagerWallet {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for BlockManagerWallet {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for BlockManagerWallet {
    fn address(&self) -> &str {
        &self.address
    }

    fn dapp_id(&self) -> &str {
        &self.dapp_id
    }
}

impl ContextAccessor for BlockManagerWallet {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for BlockManagerWallet {}

impl Executor for BlockManagerWallet {}

impl AsyncGuarded<Account> for BlockManagerWallet {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for BlockManagerWallet {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetDetails {
    pub pubkey: String,
    pub root: String,
    pub balance: String,
    pub license_num: Option<String>,
    #[serde(rename = "minstake", deserialize_with = "deserialize_u128")]
    pub min_stake: u128,
    #[serde(rename = "signerPubkey")]
    pub signer_pubkey: String,
}

impl BlockManagerWallet {
    /// Wrapper for a deployed wallet, under the all-zero system dApp.
    /// Use [`Self::with_dapp_id`] to override.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self::with_dapp_id(context, address, crate::dapp::SystemDapp::System)
    }

    /// Like [`Self::new`] but with a caller-supplied dApp ID.
    pub fn with_dapp_id(
        context: Arc<ClientContext>,
        address: impl AsRef<str>,
        dapp_id: impl Into<String>,
    ) -> Self {
        let params = crate::account::ParamsOfNewContract::new(address.as_ref(), dapp_id);
        Self {
            context: context.clone(),
            address: params.address.clone(),
            dapp_id: params.dapp_id.clone(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, &params.address, params.dapp_id))),
        }
    }

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }
}

#[cfg(test)]
mod tests {
    use crate::bksystem::bm_wallet::BlockManagerWallet;
    use crate::tests::create_context;

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn test_get_details() {
        let context = create_context();

        let bm_wallet = BlockManagerWallet::new(
            context,
            "0:0e4e5c47410d8d4e06e7be27f5a9f09e26d50852d2eaaa0c11a3d69552de0ef3",
        );

        let details = bm_wallet
            .get_details()
            .await
            .inspect_err(|e| eprintln!("Get BM wallet details ({e:?})"));
        assert!(details.is_ok());
    }
}
