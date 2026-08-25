use serde::Deserialize;
use serde::Serialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;

use crate::account::Account;
use crate::delivery::ContractContext;
use crate::deserialize::deserialize_u64;
use crate::error::KitModule;
use crate::error::TokenModule;
use crate::token::wallet::TransactionType;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/token/Transaction.abi.json");

#[derive(Debug, Clone)]
pub struct TokenTransaction {
    base: ContractBase,
}

impl ModuleAccessor for TokenTransaction {
    const MODULE: KitModule = KitModule::Token(TokenModule::Transaction);
}

impl HasContractBase for TokenTransaction {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for TokenTransaction {}

impl AsyncGuarded<Account> for TokenTransaction {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for TokenTransaction {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
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
    pub fn new(
        context: impl Into<ContractContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())) }
    }

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }
}
