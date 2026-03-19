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
use crate::accumulator::events::DecodedAccumulatorRootEvent;
use crate::accumulator::is_unsupported_created_at_filter_error;
use crate::accumulator::is_valid_denom;
use crate::accumulator::shell_sell_order_lot::ShellSellOrderLot;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;
use crate::error::AccumulatorModule;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::event::query_events as query_external_events;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::AutoContract;
use crate::traits::ContextAccessor;
use crate::traits::ContractBase;
use crate::traits::FromEvent;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/accumulator/ShellAccumulatorRootUSDC.abi.json");
const ROOT_EVENT_KIND_COUNT: usize = 4;
const ROOT_EVENT_PREFETCH_PER_KIND: usize = 2;

#[derive(Debug, Clone)]
/// Wrapper for the accumulator root `ShellAccumulatorRootUSDC` contract.
pub struct ShellAccumulatorRootUsdc {
    base: ContractBase,
}

impl ModuleAccessor for ShellAccumulatorRootUsdc {
    const MODULE: KitModule = KitModule::Accumulator(AccumulatorModule::ShellAccumulatorRootUsdc);
}

impl HasContractBase for ShellAccumulatorRootUsdc {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for ShellAccumulatorRootUsdc {}

impl AsyncGuarded<Account> for ShellAccumulatorRootUsdc {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for ShellAccumulatorRootUsdc {
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
/// Parameters for `ShellAccumulatorRootUSDC.claimUSDC`.
pub struct ParamsOfClaimUsdc {
    #[serde(rename(serialize = "D"))]
    pub d: u16,
    #[serde(rename(serialize = "orderId"))]
    pub order_id: u64,
    pub seller: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `ShellAccumulatorRootUSDC.setPubkey`.
pub struct ParamsOfSetPubkey {
    /// `uint256` encoded as decimal or hex string.
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `ShellAccumulatorRootUSDC.getQueueState`.
pub struct ParamsOfGetQueueState {
    #[serde(rename(serialize = "D"))]
    pub d: u16,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `ShellAccumulatorRootUSDC.getQueueState`.
pub struct ResultOfGetQueueState {
    #[serde(rename = "nextId", deserialize_with = "deserialize_u64")]
    pub next_id: u64,
    #[serde(deserialize_with = "deserialize_u64")]
    pub available: u64,
    #[serde(rename = "soldPrefix", deserialize_with = "deserialize_u64")]
    pub sold_prefix: u64,
    #[serde(rename = "owedCount", deserialize_with = "deserialize_u64")]
    pub owed_count: u64,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `ShellAccumulatorRootUSDC.getDetails`.
pub struct ResultOfGetDetails {
    /// `uint256` represented as returned by ABI.
    #[serde(rename = "ownerPubkey")]
    pub owner_pubkey: String,
    #[serde(rename = "sellerShellPool", deserialize_with = "deserialize_u128")]
    pub seller_shell_pool: u128,
    #[serde(rename = "usdcBalance", deserialize_with = "deserialize_u128")]
    pub usdc_balance: u128,
    #[serde(rename = "owedTotal", deserialize_with = "deserialize_u128")]
    pub owed_total: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `ShellAccumulatorRootUSDC.getSellOrderAddress`.
pub struct ParamsOfGetSellOrderAddress {
    #[serde(rename(serialize = "D"))]
    pub d: u16,
    #[serde(rename(serialize = "orderId"))]
    pub order_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `ShellAccumulatorRootUSDC.getSellOrderAddress`.
pub struct ResultOfGetSellOrderAddress {
    #[serde(rename = "sellOrderAddr")]
    pub sell_order_addr: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of scalar `uint128` getters.
pub struct ResultOfGetU128Value {
    #[serde(rename = "value0", deserialize_with = "deserialize_u128")]
    pub value: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `ShellAccumulatorRootUSDC.getNacklInfo`.
pub struct ResultOfGetNacklInfo {
    #[serde(deserialize_with = "deserialize_u128")]
    pub supply: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub burned: u128,
    #[serde(deserialize_with = "deserialize_u32")]
    pub unixstart: u32,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `ShellAccumulatorRootUSDC.updateCode`.
pub struct ParamsOfUpdateCode {
    pub newcode: String,
    pub cell: String,
}

#[derive(Debug, Clone, Serialize)]
/// Query params for accumulator root external events.
pub struct ParamsOfQueryAccumulatorRootEvents {
    /// Lower bound (inclusive) for event timestamp.
    pub created_at_from: Option<u64>,
    /// Max number of decoded items to return.
    pub limit: Option<u32>,
}

impl Default for ParamsOfQueryAccumulatorRootEvents {
    fn default() -> Self {
        Self { created_at_from: None, limit: Some(50) }
    }
}

impl ShellAccumulatorRootUsdc {
    /// Create a wrapper for a deployed `ShellAccumulatorRootUSDC`.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// Original contract method: `claimUSDC`.
    pub async fn claim_usdc(
        &self,
        params: ParamsOfClaimUsdc,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "claimUSDC".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `setPubkey`.
    pub async fn set_pubkey(
        &self,
        params: ParamsOfSetPubkey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setPubkey".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `getQueueState`.
    pub async fn get_queue_state(
        &self,
        params: ParamsOfGetQueueState,
    ) -> KitResult<ResultOfGetQueueState> {
        self.call_get_method_with::<ResultOfGetQueueState, ParamsOfGetQueueState>(
            "getQueueState",
            params,
        )
        .await
    }

    /// Original contract method: `getDetails`.
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// Original contract method: `getSellOrderAddress`.
    pub async fn get_sell_order_address(
        &self,
        params: ParamsOfGetSellOrderAddress,
    ) -> KitResult<ResultOfGetSellOrderAddress> {
        self.call_get_method_with::<ResultOfGetSellOrderAddress, ParamsOfGetSellOrderAddress>(
            "getSellOrderAddress",
            params,
        )
        .await
    }

    /// Convenience helper for seller claim flow:
    /// 1. Resolve `ShellSellOrderLot` address by `(D, orderId)`.
    /// 2. Call `ShellSellOrderLot.claim()` with provided signer.
    pub async fn claim_for_order(
        &self,
        d: u16,
        order_id: u64,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        if !is_valid_denom(d) {
            return Err(KitError::new(
                Self::MODULE,
                KitErrorCode::InvalidInput,
                format!("Invalid denomination `{d}`. Expected one of: 1, 10, 100, 1000"),
            ));
        }

        let sell_order_addr = self
            .get_sell_order_address(ParamsOfGetSellOrderAddress { d, order_id })
            .await?
            .sell_order_addr;
        let sell_order_lot = ShellSellOrderLot::new(self.context().clone(), &sell_order_addr);
        sell_order_lot.claim(signer).await
    }

    /// Original contract method: `owedUsdcTotal`.
    pub async fn owed_usdc_total(&self) -> KitResult<ResultOfGetU128Value> {
        self.call_get_method::<ResultOfGetU128Value>("owedUsdcTotal").await
    }

    /// Original contract method: `getSellerShellPool`.
    pub async fn get_seller_shell_pool(&self) -> KitResult<ResultOfGetU128Value> {
        self.call_get_method::<ResultOfGetU128Value>("getSellerShellPool").await
    }

    /// Original contract method: `getUsdcBalance`.
    pub async fn get_usdc_balance(&self) -> KitResult<ResultOfGetU128Value> {
        self.call_get_method::<ResultOfGetU128Value>("getUsdcBalance").await
    }

    /// Original contract method: `getNacklInfo`.
    pub async fn get_nackl_info(&self) -> KitResult<ResultOfGetNacklInfo> {
        self.call_get_method::<ResultOfGetNacklInfo>("getNacklInfo").await
    }

    /// Query and decode external events emitted by accumulator root.
    ///
    /// Events are queried by `(src = self.address)` and decoded into typed
    /// payloads. Unknown external messages are ignored.
    pub async fn query_events(
        &self,
        params: ParamsOfQueryAccumulatorRootEvents,
    ) -> KitResult<Vec<DecodedAccumulatorRootEvent>> {
        let created_at_from = params.created_at_from.unwrap_or_default();
        let limit = params.limit.unwrap_or(50) as usize;
        // Prefetch with slack to compensate for mixed event types and decode filtering.
        let prefetch_limit = limit
            .saturating_mul(ROOT_EVENT_KIND_COUNT)
            .saturating_mul(ROOT_EVENT_PREFETCH_PER_KIND);
        let src_only_filter = json!({
            "src": { "eq": self.address() },
        });
        let with_created_at_filter = if created_at_from == 0 {
            src_only_filter.clone()
        } else {
            json!({
                "src": { "eq": self.address() },
                "created_at": { "ge": created_at_from },
            })
        };
        let raw_events = match query_external_events(
            self.context().clone(),
            Some(with_created_at_filter),
            None,
            Some(prefetch_limit as u32),
        )
        .await
        {
            Ok(events) => events,
            // TODO(shellnet-index): confirm current indexer/GraphQL supports
            // `messages.created_at: { ge: ... }` inside `query_collection` filter.
            // Fallback strategy (kept here intentionally): retry with src-only query
            // and apply `created_at_from` cutoff locally in Rust.
            Err(err) if created_at_from > 0 && is_unsupported_created_at_filter_error(&err) => {
                query_external_events(
                    self.context().clone(),
                    Some(src_only_filter),
                    None,
                    Some(prefetch_limit as u32),
                )
                .await?
            }
            Err(err) => return Err(err),
        };
        let mut decoded_events = Vec::new();
        for event in raw_events {
            if event.created_at < created_at_from {
                continue;
            }
            match DecodedAccumulatorRootEvent::from_event(&event, self) {
                Ok(decoded) => decoded_events.push(decoded),
                Err(error)
                    if matches!(error.code, KitErrorCode::UnknownEvent | KitErrorCode::Parse) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        decoded_events.sort_by(|left, right| {
            right
                .event()
                .created_at
                .cmp(&left.event().created_at)
                .then_with(|| right.event().id.cmp(&left.event().id))
        });
        decoded_events.truncate(limit);
        Ok(decoded_events)
    }

    /// Original contract method: `updateCode`.
    pub async fn update_code(
        &self,
        params: ParamsOfUpdateCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "updateCode".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
