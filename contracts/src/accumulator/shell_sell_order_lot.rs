use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::accumulator::events::DecodedSellOrderLotEvent;
use crate::deserialize::deserialize_u16;
use crate::deserialize::deserialize_u64;
use crate::error::AccumulatorModule;
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

const ABI: &str = include_str!("../../abi/accumulator/ShellSellOrderLot.abi.json");
const SELL_ORDER_LOT_EVENT_KIND_COUNT: usize = 2;
const SELL_ORDER_LOT_EVENT_PREFETCH_PER_KIND: usize = 3;

#[derive(Debug, Clone)]
/// Wrapper for the `ShellSellOrderLot` contract.
pub struct ShellSellOrderLot {
    base: ContractBase,
}

impl ModuleAccessor for ShellSellOrderLot {
    const MODULE: KitModule = KitModule::Accumulator(AccumulatorModule::ShellSellOrderLot);
}

impl HasContractBase for ShellSellOrderLot {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for ShellSellOrderLot {}

impl AsyncGuarded<Account> for ShellSellOrderLot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for ShellSellOrderLot {
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
/// Result of `ShellSellOrderLot.getDetails`.
pub struct ResultOfGetDetails {
    pub root: String,
    pub owner: String,
    #[serde(deserialize_with = "deserialize_u16")]
    pub denom: u16,
    #[serde(rename = "orderId", deserialize_with = "deserialize_u64")]
    pub order_id: u64,
    pub claimed: bool,
}

#[derive(Debug, Clone, Serialize)]
/// Query params for sell-order-lot external events.
pub struct ParamsOfQuerySellOrderLotEvents {
    /// Lower bound (inclusive) for event timestamp.
    pub created_at_from: Option<u64>,
    /// Max number of decoded items to return.
    pub limit: Option<u32>,
}

impl Default for ParamsOfQuerySellOrderLotEvents {
    fn default() -> Self {
        Self { created_at_from: None, limit: Some(50) }
    }
}

impl ShellSellOrderLot {
    /// Create a wrapper for a deployed `ShellSellOrderLot`.
    pub fn new(
        context: Arc<ClientContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        let params = params.into();
        Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())) }
    }

    /// Original contract method: `claim`.
    pub async fn claim(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet { function_name: "claim".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `getDetails`.
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// Query and decode external events emitted by this sell-order-lot.
    ///
    /// Events are queried by `(src = self.address)` and decoded by event name
    /// from message BOC. Unknown external messages are ignored.
    pub async fn query_events(
        &self,
        params: ParamsOfQuerySellOrderLotEvents,
    ) -> KitResult<Vec<DecodedSellOrderLotEvent>> {
        let created_at_from = params.created_at_from.unwrap_or_default();
        let limit = params.limit.unwrap_or(50) as usize;
        let prefetch_limit = limit
            .saturating_mul(SELL_ORDER_LOT_EVENT_KIND_COUNT)
            .saturating_mul(SELL_ORDER_LOT_EVENT_PREFETCH_PER_KIND);
        let raw_events = query_external_events(
            self.context().clone(),
            self.address(),
            self.dapp_id(),
            Some(prefetch_limit as u32),
        )
        .await?;
        let mut decoded_events = Vec::new();
        for event in raw_events {
            if event.created_at < created_at_from {
                continue;
            }
            match DecodedSellOrderLotEvent::from_event(&event, self) {
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
}
