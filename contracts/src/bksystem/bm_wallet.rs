use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use serde::Deserialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u128;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::EncodeMessage;
use crate::traits::Executor;

const ABI: &str = include_str!("../../abi/bksystem/AckiNackiBlockManagerNodeWallet.abi.json");

#[derive(Debug, Clone)]
pub struct BlockManagerWallet {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
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
}

impl ContextAccessor for BlockManagerWallet {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for BlockManagerWallet {}

impl Executor for BlockManagerWallet {}

#[async_trait]
impl AsyncGuarded<Account> for BlockManagerWallet {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T + Send + 'async_trait,
        T: Send + 'async_trait,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

#[async_trait]
impl AsyncGuardedMut<Account> for BlockManagerWallet {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut + Send + 'async_trait,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'async_trait,
        T: Send + 'async_trait,
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
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    pub async fn get_details(&self) -> anyhow::Result<ResultOfGetDetails> {
        let call_set =
            CallSet { function_name: "getDetails".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetDetails>(value)
                    .map_err(|e| anyhow!("Deserialize output ({})", e)),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::bksystem::bm_wallet::BlockManagerWallet;
    use crate::tests::create_context;

    #[tokio::test]
    async fn test_get_details() {
        let context = create_context();

        let bm_wallet = BlockManagerWallet::new(
            context,
            "0:0e4e5c47410d8d4e06e7be27f5a9f09e26d50852d2eaaa0c11a3d69552de0ef3",
        );

        let details =
            bm_wallet.get_details().await.inspect_err(|e| eprintln!("Get BM wallet details ({e})"));
        assert!(details.is_ok());
    }
}
