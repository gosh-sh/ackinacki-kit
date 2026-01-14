use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u64;
use crate::error::KitModule;
use crate::error::TokenModule;
use crate::token::wallet::TransactionType;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::GetMethodAccessor;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/token/Transaction.abi.json");

#[derive(Debug, Clone)]
pub struct TokenTransaction {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for TokenTransaction {
    const MODULE: KitModule = KitModule::Token(TokenModule::Transaction);
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

impl AsyncGuarded<Account> for TokenTransaction {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for TokenTransaction {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
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

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }
}
