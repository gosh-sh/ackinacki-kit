use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::bksystem::LicenseData;
use crate::bksystem::Stake;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u8;
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

const ABI: &str = include_str!("../../abi/bksystem/AckiNackiBlockKeeperNodeWallet.abi.json");

#[derive(Debug, Clone)]
pub struct BlockKeeperWallet {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for BlockKeeperWallet {
    const MODULE: KitModule = KitModule::BkSystem(BkSystemModule::BlockKeeperWallet);
}

impl AccountAccessor for BlockKeeperWallet {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for BlockKeeperWallet {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for BlockKeeperWallet {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for BlockKeeperWallet {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for BlockKeeperWallet {}

impl Executor for BlockKeeperWallet {}

impl AsyncGuarded<Account> for BlockKeeperWallet {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for BlockKeeperWallet {
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
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
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
            "0:733e033541ad17c4251cdf97378045e44d8eb89ddfe4659cf5b45e4376a3a02e",
        );

        let details = bk_wallet
            .get_details()
            .await
            .inspect_err(|e| eprintln!("Get BK wallet details ({e:?})"));
        assert!(details.is_ok());
    }
}
