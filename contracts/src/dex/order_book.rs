use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;
use crate::deserialize::deserialize_u8;
use crate::error::DexModule;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/dex/OrderBook.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for the DEX `OrderBook` contract.
pub struct OrderBook {
    base: ContractBase,
}

impl ModuleAccessor for OrderBook {
    const MODULE: KitModule = KitModule::Dex(DexModule::OrderBook);
}

impl HasContractBase for OrderBook {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for OrderBook {}

impl AsyncGuarded<Account> for OrderBook {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for OrderBook {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `OrderBook.execute`.
pub struct ParamsOfExecute {
    #[serde(rename(serialize = "actionType"))]
    pub action_type: u8,
    #[serde(rename(serialize = "outcomeId"))]
    pub outcome_id: u32,
    #[serde(rename(serialize = "isBuy"))]
    pub is_buy: bool,
    #[serde(rename(serialize = "priceBps"))]
    pub price_bps: u128,
    pub amount: u128,
    #[serde(rename(serialize = "orderId"))]
    pub order_id: u128,
    pub deposit_identifier_hash: String,
    pub flags: u8,
    #[serde(rename(serialize = "minAmount"))]
    pub min_amount: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `OrderBook.getDetails`.
pub struct ResultOfGetDetails {
    pub event_id: String,
    pub oracle_list_hash: String,
    #[serde(rename = "token_type", deserialize_with = "deserialize_u32")]
    pub token_type: u32,
    #[serde(rename = "epochDuration", deserialize_with = "deserialize_u64")]
    pub epoch_duration: u64,
    #[serde(rename = "currentEpochStart", deserialize_with = "deserialize_u64")]
    pub current_epoch_start: u64,
    #[serde(rename = "nextOrderId", deserialize_with = "deserialize_u128")]
    pub next_order_id: u128,
    #[serde(rename = "orderCount", deserialize_with = "deserialize_u128")]
    pub order_count: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `OrderBook.getOrder`.
pub struct ParamsOfGetOrder {
    #[serde(rename(serialize = "orderId"))]
    pub order_id: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `OrderBook.getOrder`.
pub struct ResultOfGetOrder {
    pub deposit_identifier_hash: String,
    #[serde(rename = "outcomeId", deserialize_with = "deserialize_u32")]
    pub outcome_id: u32,
    #[serde(rename = "isBuy")]
    pub is_buy: bool,
    #[serde(deserialize_with = "deserialize_u8")]
    pub flags: u8,
    #[serde(rename = "priceBps", deserialize_with = "deserialize_u128")]
    pub price_bps: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub amount: u128,
    #[serde(rename = "minAmount", deserialize_with = "deserialize_u128")]
    pub min_amount: u128,
    #[serde(rename = "epochId", deserialize_with = "deserialize_u64")]
    pub epoch_id: u64,
}

impl OrderBook {
    /// Create a wrapper for a deployed `OrderBook`.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// # Execute OrderBook action
    ///
    /// Original contract method: `execute`
    pub async fn execute(
        &self,
        params: ParamsOfExecute,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "execute".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Get OrderBook details
    ///
    /// Original contract method: `getDetails`
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// # Get order details by id
    ///
    /// Original contract method: `getOrder`
    pub async fn get_order(&self, params: ParamsOfGetOrder) -> KitResult<ResultOfGetOrder> {
        self.call_get_method_with::<ResultOfGetOrder, ParamsOfGetOrder>("getOrder", params).await
    }
}
