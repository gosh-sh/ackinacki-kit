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

// ─── Order tuple used by `executeBatch.orders[]` ───────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// One element of the `orders` array passed to `OrderBook.executeBatch`.
/// Mirrors the on-chain tuple layout exactly.
pub struct OrderBookOrder {
    pub outcome_id: u32,
    pub is_buy: bool,
    pub flags: u8,
    /// `uint256`, decimal or hex string.
    pub price: String,
    pub amount: u128,
    pub min_amount: u128,
    pub epoch_id: u64,
    pub client_order_id: u128,
}

// ─── Method param structs ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OrderBook.setResultStart`.
pub struct ParamsOfSetResultStart {
    pub result_start: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OrderBook.executeBatch`.
pub struct ParamsOfExecuteBatch {
    pub deposit_identifier_hash: String,
    pub orders: Vec<OrderBookOrder>,
    pub cancel_ids: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OrderBook.cancelAllOrders`.
pub struct ParamsOfCancelAllOrders {
    pub deposit_identifier_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OrderBook.cancelQueued`.
pub struct ParamsOfCancelQueued {
    pub slot: u8,
    pub queue_id: u32,
    pub deposit_identifier_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OrderBook.cancelByClientId`.
pub struct ParamsOfCancelByClientId {
    pub deposit_identifier_hash: String,
    pub client_order_id: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OrderBook.getOrder`.
pub struct ParamsOfGetOrder {
    pub order_id: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OrderBook.getOrdersByOwner`.
pub struct ParamsOfGetOrdersByOwner {
    pub deposit_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OrderBook.getOrderIdByClient`.
pub struct ParamsOfGetOrderIdByClient {
    pub deposit_hash: String,
    pub client_order_id: u128,
}

// ─── Result structs ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Result of `OrderBook.getDetails`.
pub struct ResultOfGetDetails {
    pub event_id: String,
    pub oracle_list_hash: String,
    #[serde(deserialize_with = "deserialize_u32")]
    pub token_type: u32,
    #[serde(deserialize_with = "deserialize_u128")]
    pub next_order_id: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub order_count: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub total_maker_fees: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub total_taker_fees: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `OrderBook.getQueueSize`.
pub struct ResultOfGetQueueSize {
    #[serde(deserialize_with = "deserialize_u8")]
    pub size: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Result of `OrderBook.getOrder`.
pub struct ResultOfGetOrder {
    pub deposit_identifier_hash: String,
    #[serde(deserialize_with = "deserialize_u32")]
    pub outcome_id: u32,
    pub is_buy: bool,
    #[serde(deserialize_with = "deserialize_u8")]
    pub flags: u8,
    /// `uint256` represented as returned by ABI.
    pub price: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub amount: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub min_amount: u128,
    #[serde(deserialize_with = "deserialize_u64")]
    pub epoch_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Result of `OrderBook.getOrdersByOwner`.
pub struct ResultOfGetOrdersByOwner {
    pub order_ids: Vec<String>,
    pub outcome_ids: Vec<String>,
    pub is_buys: Vec<bool>,
    /// `uint256[]` returned as decimal/hex strings.
    pub prices: Vec<String>,
    pub amounts: Vec<String>,
    pub epoch_ids: Vec<String>,
    pub client_order_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Result of `OrderBook.getOrderIdByClient`.
pub struct ResultOfGetOrderIdByClient {
    #[serde(deserialize_with = "deserialize_u128")]
    pub order_id: u128,
}

// ─── Method bindings ──────────────────────────────────────────────────────

impl OrderBook {
    /// Create a wrapper for a deployed `OrderBook`.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// Original contract method: `setResultStart`.
    pub async fn set_result_start(
        &self,
        params: ParamsOfSetResultStart,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setResultStart".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `executeBatch`. Submits a batch of new
    /// orders + a list of order IDs to cancel, all bound to a single
    /// `depositIdentifierHash` (the calling PrivateNote).
    pub async fn execute_batch(
        &self,
        params: ParamsOfExecuteBatch,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "executeBatch".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `cancelAllOrders`.
    pub async fn cancel_all_orders(
        &self,
        params: ParamsOfCancelAllOrders,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "cancelAllOrders".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `cancelQueued`.
    pub async fn cancel_queued(
        &self,
        params: ParamsOfCancelQueued,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "cancelQueued".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `cancelByClientId`.
    pub async fn cancel_by_client_id(
        &self,
        params: ParamsOfCancelByClientId,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "cancelByClientId".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `processHead`. Drains the matching queue
    /// without submitting new orders.
    pub async fn process_head(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set =
            CallSet { function_name: "processHead".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `shutdown`. Owner-only.
    pub async fn shutdown(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet { function_name: "shutdown".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `getDetails`.
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// Original contract method: `getQueueSize`.
    pub async fn get_queue_size(&self) -> KitResult<ResultOfGetQueueSize> {
        self.call_get_method::<ResultOfGetQueueSize>("getQueueSize").await
    }

    /// Original contract method: `getOrder`.
    pub async fn get_order(&self, params: ParamsOfGetOrder) -> KitResult<ResultOfGetOrder> {
        self.call_get_method_with::<ResultOfGetOrder, ParamsOfGetOrder>("getOrder", params).await
    }

    /// Original contract method: `getOrdersByOwner`.
    pub async fn get_orders_by_owner(
        &self,
        params: ParamsOfGetOrdersByOwner,
    ) -> KitResult<ResultOfGetOrdersByOwner> {
        self.call_get_method_with::<ResultOfGetOrdersByOwner, ParamsOfGetOrdersByOwner>(
            "getOrdersByOwner",
            params,
        )
        .await
    }

    /// Original contract method: `getOrderIdByClient`.
    pub async fn get_order_id_by_client(
        &self,
        params: ParamsOfGetOrderIdByClient,
    ) -> KitResult<ResultOfGetOrderIdByClient> {
        self.call_get_method_with::<ResultOfGetOrderIdByClient, ParamsOfGetOrderIdByClient>(
            "getOrderIdByClient",
            params,
        )
        .await
    }
}
