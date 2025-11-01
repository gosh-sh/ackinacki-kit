use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u64;
use crate::token::wallet::TransactionType;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/token/Transaction.abi.json");

#[derive(Debug, Clone)]
pub struct TokenTransaction {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for TokenTransaction {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for TokenTransaction {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for TokenTransaction {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for TokenTransaction {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for TokenTransaction {}

impl DecodeMessage for TokenTransaction {}

impl Executor for TokenTransaction {}

impl SendMessage for TokenTransaction {}

#[async_trait]
impl AsyncGuarded<Account> for TokenTransaction {
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
impl AsyncGuardedMut<Account> for TokenTransaction {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetDetails {
    pub wallet: String,
    pub data: String,
    #[serde(rename = "transactionType")]
    pub transaction_type: TransactionType,
    #[serde(rename = "seqnoDestroy", deserialize_with = "deserialize_u64")]
    pub seqno_destroy: u64,
    #[serde(rename = "ownerAddress")]
    pub owner_address: String,
    #[serde(rename = "dataHash")]
    pub data_hash: String,
}

impl TokenTransaction {
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
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }
}
