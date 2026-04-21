use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::ParamsOfDecodeMessageBody;
use tvm_client::abi::Signer;
use tvm_client::net;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::accumulator::events::AccumulatorRootEvent;
use crate::accumulator::events::DecodedAccumulatorRootEvent;
use crate::accumulator::events::SellOrderCreatedData;
use crate::accumulator::events::UsdcClaimedData;
use crate::accumulator::is_valid_denom;
use crate::accumulator::shell_sell_order_lot::ShellSellOrderLot;
use crate::accumulator::VALID_DENOMS;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;
use crate::error::AccumulatorModule;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::event::query_events as query_external_events;
use crate::traits::AbiAccessor;
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
const ROOT_EVENT_KIND_COUNT: usize = 5;
const ROOT_EVENT_PREFETCH_PER_KIND: usize = 2;
const SELL_ORDER_CREATED_PAGE_SIZE: i32 = 100;
const GQL_ACCUMULATOR_ROOT_EVENTS_BY_DST_QUERY: &str = r#"
    query($address: String!, $dst: String!, $last: Int!, $before: String) {
      blockchain {
        account(address: $address) {
          events(dst: $dst, last: $last, before: $before) {
            edges {
              cursor
              node {
                msg_id
                created_at
                dst
                body
              }
            }
          }
        }
      }
    }
"#;

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

#[derive(Debug, Clone)]
/// Aggregated seller order status for wallet/UI usage.
pub struct SellerOrderInfo {
    pub denom: u16,
    pub order_id: u64,
    pub sell_order_address: String,
    pub claimed: bool,
    pub sold: bool,
    /// Current queue position for unsold orders (`1..N`), `0` for sold ones.
    pub position_in_queue: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ParamsOfGetOrdersBySeller {
    pub seller: String,
    /// Max items per page. Default 20.
    pub limit: Option<u32>,
    /// Opaque cursor from previous page. `None` = first page.
    pub cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResultOfGetOrdersBySeller {
    pub orders: Vec<SellerOrderInfo>,
    /// Cursor for the next page. `None` = last page.
    pub next_cursor: Option<String>,
    pub has_next_page: bool,
}

impl Default for ParamsOfQueryAccumulatorRootEvents {
    fn default() -> Self {
        Self { created_at_from: None, limit: Some(50) }
    }
}

impl ShellAccumulatorRootUsdc {
    /// Default zerostate accumulator root address.
    pub const DEFAULT_ADDRESS: &'static str =
        "0:3535353535353535353535353535353535353535353535353535353535353535";

    /// Create a wrapper for a deployed `ShellAccumulatorRootUSDC`.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// Create a wrapper bound to the default zerostate accumulator root.
    pub fn new_default(context: Arc<ClientContext>) -> Self {
        Self::new(context, Self::DEFAULT_ADDRESS)
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

    /// Returns active sell orders for `seller` using seller-directed
    /// `SellOrderCreated` events and local status derivation from queues.
    ///
    /// New accumulator contract emits `SellOrderCreated` twice:
    /// 1. to seller external address (`dest = :<seller.value>`)
    /// 2. to shared event channel (`dest = :...0262`, id=610)
    ///
    /// This method primarily uses seller-specific `dest`, which allows fetching
    /// orders for one seller without scanning global sell-order history.
    /// Returns paginated sell orders for `seller`.
    pub async fn get_orders_by_seller(
        &self,
        params: ParamsOfGetOrdersBySeller,
    ) -> KitResult<ResultOfGetOrdersBySeller> {
        let seller = &params.seller;
        let limit = params.limit.unwrap_or(20).max(1) as usize;
        let after = params.cursor.as_deref().map(decode_cursor).transpose()?;

        let created_orders = self.query_created_orders_by_seller(seller).await?;
        let claimed_orders = self.query_claimed_orders(seller).await?;

        let mut queue_states = HashMap::new();
        for denom in VALID_DENOMS {
            let state = self.get_queue_state(ParamsOfGetQueueState { d: denom }).await?;
            queue_states.insert(denom, state);
        }

        // Build sorted candidate list (cheap — no RPC per item).
        let mut candidates: Vec<(u16, u64)> = Vec::new();
        for (denom, order_id) in created_orders {
            if claimed_orders.contains(&(denom, order_id)) {
                continue;
            }
            let Some(queue_state) = queue_states.get(&denom) else {
                continue;
            };
            if order_id == 0 || order_id >= queue_state.next_id {
                continue;
            }
            candidates.push((denom, order_id));
        }
        candidates.sort();

        // Skip past cursor.
        let start = match after {
            Some(cursor_key) => {
                candidates.iter().position(|k| *k > cursor_key).unwrap_or(candidates.len())
            }
            None => 0,
        };

        // Take limit+1 to detect next page.
        let page_candidates = &candidates[start..candidates.len().min(start + limit + 1)];

        // Fetch details only for this page (expensive part).
        let mut result = Vec::new();
        for &(denom, order_id) in page_candidates {
            if result.len() > limit {
                break;
            }

            let queue_state = &queue_states[&denom];
            let sold = order_id <= queue_state.sold_prefix;
            let position_in_queue =
                if sold { 0 } else { order_id.saturating_sub(queue_state.sold_prefix) };

            let sell_order_address = self
                .get_sell_order_address(ParamsOfGetSellOrderAddress { d: denom, order_id })
                .await?
                .sell_order_addr;

            let sell_order_lot =
                ShellSellOrderLot::new(self.context().clone(), &sell_order_address);
            let details = match sell_order_lot.get_details().await {
                Ok(d) => d,
                Err(_) => continue,
            };
            if !addresses_equal(&details.owner, seller) {
                continue;
            }

            result.push(SellerOrderInfo {
                denom,
                order_id,
                sell_order_address,
                claimed: details.claimed,
                sold,
                position_in_queue,
            });
        }

        let has_next_page = result.len() > limit;
        if has_next_page {
            result.truncate(limit);
        }

        let next_cursor = if has_next_page {
            result.last().map(|o| encode_cursor(o.denom, o.order_id))
        } else {
            None
        };

        Ok(ResultOfGetOrdersBySeller { orders: result, next_cursor, has_next_page })
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
        let prefetch_limit = limit
            .saturating_mul(ROOT_EVENT_KIND_COUNT)
            .saturating_mul(ROOT_EVENT_PREFETCH_PER_KIND);
        let raw_events = query_external_events(
            self.context().clone(),
            self.address(),
            Some(prefetch_limit as u32),
        )
        .await?;
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

    async fn query_created_orders_by_seller(&self, seller: &str) -> KitResult<Vec<(u16, u64)>> {
        let seller_normalized = normalize_address(seller);
        let seller_dst = internal_to_external_address(seller);
        let by_seller_dst =
            self.query_created_orders_by_dst(&seller_dst, &seller_normalized).await?;
        if !by_seller_dst.is_empty() {
            return Ok(by_seller_dst);
        }

        // Backward compatibility for older accumulator deployments where
        // `SellOrderCreated` was emitted only to fixed external id `610`.
        let legacy_dst = AccumulatorRootEvent::SellOrderCreated.to_external_address();
        self.query_created_orders_by_dst(&legacy_dst, &seller_normalized).await
    }

    async fn query_created_orders_by_dst(
        &self,
        dst: &str,
        seller_normalized: &str,
    ) -> KitResult<Vec<(u16, u64)>> {
        let mut before: Option<String> = None;
        let mut seen = BTreeSet::<(u16, u64)>::new();

        loop {
            let raw = net::query(
                self.context().clone(),
                net::ParamsOfQuery {
                    query: GQL_ACCUMULATOR_ROOT_EVENTS_BY_DST_QUERY.to_string(),
                    variables: Some(json!({
                        "address": self.address(),
                        "dst": dst,
                        "last": SELL_ORDER_CREATED_PAGE_SIZE,
                        "before": before,
                    })),
                },
            )
            .await
            .map_err(|e| {
                KitError::new(
                    Self::MODULE,
                    KitErrorCode::QueryEvents,
                    "Query SellOrderCreated events with GraphQL",
                )
                .with_tvm_error(e)
            })?;

            let parsed: GqlMessagesResponse = serde_json::from_value(raw.result).map_err(|e| {
                KitError::new(
                    Self::MODULE,
                    KitErrorCode::DeserializeFailed,
                    format!("Deserialize SellOrderCreated GraphQL response ({e})"),
                )
            })?;

            let edges = parsed.data.blockchain.account.events.edges;
            if edges.is_empty() {
                break;
            }

            let next_before = edges.first().map(|edge| edge.cursor.clone());
            for edge in edges {
                let node = edge.node;
                let decoded = tvm_client::abi::decode_message_body(
                    self.context().clone(),
                    ParamsOfDecodeMessageBody {
                        abi: self.abi().clone(),
                        body: node.body,
                        is_internal: false,
                        allow_partial: true,
                        function_name: None,
                        data_layout: None,
                    },
                )
                .map_err(|e| {
                    KitError::new(
                        Self::MODULE,
                        KitErrorCode::Decode,
                        "Decode SellOrderCreated body",
                    )
                    .with_tvm_error(e)
                })?;

                if decoded.name != "SellOrderCreated" {
                    continue;
                }

                let raw_value = decoded.value.ok_or_else(|| {
                    KitError::new(
                        Self::MODULE,
                        KitErrorCode::EmptyData,
                        "Empty SellOrderCreated payload",
                    )
                })?;
                let data =
                    serde_json::from_value::<SellOrderCreatedData>(raw_value).map_err(|e| {
                        KitError::new(
                            Self::MODULE,
                            KitErrorCode::DeserializeFailed,
                            format!("Deserialize SellOrderCreated payload ({e})"),
                        )
                    })?;

                if normalize_address(&data.seller) == seller_normalized {
                    seen.insert((data.denom, data.order_id));
                }
            }

            match (before.as_ref(), next_before) {
                (_, None) => break,
                (Some(current), Some(next)) if current == &next => break,
                (_, Some(next)) => before = Some(next),
            }
        }

        Ok(seen.into_iter().collect())
    }

    /// Queries `UsdcClaimed` events emitted by the root contract and returns
    /// the set of `(denom, order_id)` pairs that have been fully claimed by `seller`.
    async fn query_claimed_orders(&self, seller: &str) -> KitResult<BTreeSet<(u16, u64)>> {
        let seller_normalized = normalize_address(seller);
        let dst = AccumulatorRootEvent::UsdcClaimed.to_external_address();
        let mut before: Option<String> = None;
        let mut claimed = BTreeSet::<(u16, u64)>::new();

        loop {
            let raw = net::query(
                self.context().clone(),
                net::ParamsOfQuery {
                    query: GQL_ACCUMULATOR_ROOT_EVENTS_BY_DST_QUERY.to_string(),
                    variables: Some(json!({
                        "address": self.address(),
                        "dst": dst,
                        "last": SELL_ORDER_CREATED_PAGE_SIZE,
                        "before": before,
                    })),
                },
            )
            .await
            .map_err(|e| {
                KitError::new(
                    Self::MODULE,
                    KitErrorCode::QueryEvents,
                    "Query UsdcClaimed events with GraphQL",
                )
                .with_tvm_error(e)
            })?;

            let parsed: GqlMessagesResponse = serde_json::from_value(raw.result).map_err(|e| {
                KitError::new(
                    Self::MODULE,
                    KitErrorCode::DeserializeFailed,
                    format!("Deserialize UsdcClaimed GraphQL response ({e})"),
                )
            })?;

            let edges = parsed.data.blockchain.account.events.edges;
            if edges.is_empty() {
                break;
            }

            let next_before = edges.first().map(|edge| edge.cursor.clone());
            for edge in edges {
                let node = edge.node;
                let decoded = tvm_client::abi::decode_message_body(
                    self.context().clone(),
                    ParamsOfDecodeMessageBody {
                        abi: self.abi().clone(),
                        body: node.body,
                        is_internal: false,
                        allow_partial: true,
                        function_name: None,
                        data_layout: None,
                    },
                )
                .map_err(|e| {
                    KitError::new(Self::MODULE, KitErrorCode::Decode, "Decode UsdcClaimed body")
                        .with_tvm_error(e)
                })?;

                if decoded.name != "UsdcClaimed" {
                    continue;
                }

                let raw_value = decoded.value.ok_or_else(|| {
                    KitError::new(
                        Self::MODULE,
                        KitErrorCode::EmptyData,
                        "Empty UsdcClaimed payload",
                    )
                })?;
                let data = serde_json::from_value::<UsdcClaimedData>(raw_value).map_err(|e| {
                    KitError::new(
                        Self::MODULE,
                        KitErrorCode::DeserializeFailed,
                        format!("Deserialize UsdcClaimed payload ({e})"),
                    )
                })?;

                if normalize_address(&data.seller) == seller_normalized {
                    claimed.insert((data.denom, data.order_id));
                }
            }

            match (before.as_ref(), next_before) {
                (_, None) => break,
                (Some(current), Some(next)) if current == &next => break,
                (_, Some(next)) => before = Some(next),
            }
        }

        Ok(claimed)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GqlMessagesResponse {
    data: GqlMessagesData,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlMessagesData {
    blockchain: GqlBlockchain,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlBlockchain {
    account: GqlAccount,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlAccount {
    events: GqlEvents,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEvents {
    edges: Vec<GqlEdge>,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEdge {
    cursor: String,
    node: GqlEventNode,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEventNode {
    #[serde(rename = "msg_id")]
    _msg_id: String,
    #[serde(rename = "created_at")]
    _created_at: u64,
    #[serde(rename = "dst")]
    _dst: String,
    body: String,
}

fn encode_cursor(denom: u16, order_id: u64) -> String {
    base64::engine::general_purpose::STANDARD.encode(format!("{denom}:{order_id}"))
}

fn decode_cursor(cursor: &str) -> KitResult<(u16, u64)> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(cursor).map_err(|e| {
        KitError::new(
            KitModule::Accumulator(AccumulatorModule::ShellAccumulatorRootUsdc),
            KitErrorCode::InvalidInput,
            format!("Invalid cursor ({e})"),
        )
    })?;
    let s = String::from_utf8(bytes).map_err(|e| {
        KitError::new(
            KitModule::Accumulator(AccumulatorModule::ShellAccumulatorRootUsdc),
            KitErrorCode::InvalidInput,
            format!("Invalid cursor ({e})"),
        )
    })?;
    let (denom_str, order_id_str) = s.split_once(':').ok_or_else(|| {
        KitError::new(
            KitModule::Accumulator(AccumulatorModule::ShellAccumulatorRootUsdc),
            KitErrorCode::InvalidInput,
            format!("Invalid cursor format: {s}"),
        )
    })?;
    let denom = denom_str.parse::<u16>().map_err(|e| {
        KitError::new(
            KitModule::Accumulator(AccumulatorModule::ShellAccumulatorRootUsdc),
            KitErrorCode::InvalidInput,
            format!("Invalid cursor denom ({e})"),
        )
    })?;
    let order_id = order_id_str.parse::<u64>().map_err(|e| {
        KitError::new(
            KitModule::Accumulator(AccumulatorModule::ShellAccumulatorRootUsdc),
            KitErrorCode::InvalidInput,
            format!("Invalid cursor order_id ({e})"),
        )
    })?;
    Ok((denom, order_id))
}

fn normalize_address(address: &str) -> String {
    address
        .strip_prefix("0x")
        .or_else(|| address.strip_prefix("0X"))
        .or_else(|| address.strip_prefix("0:"))
        .or_else(|| address.strip_prefix(':'))
        .unwrap_or(address)
        .to_ascii_lowercase()
}

fn internal_to_external_address(address: &str) -> String {
    format!(":{}", normalize_address(address))
}

fn addresses_equal(left: &str, right: &str) -> bool {
    normalize_address(left) == normalize_address(right)
}

#[cfg(test)]
mod tests {
    use super::addresses_equal;
    use super::decode_cursor;
    use super::encode_cursor;
    use super::internal_to_external_address;

    #[test]
    fn addresses_equal_accepts_common_prefix_forms() {
        assert!(addresses_equal(
            "0:12f6b8eeec7e417f9b56ed3635aed523d362a1aabe504ae4731d97c03a4ed60c",
            ":12f6b8eeec7e417f9b56ed3635aed523d362a1aabe504ae4731d97c03a4ed60c",
        ));
        assert!(addresses_equal(
            "0x12F6B8EEEC7E417F9B56ED3635AED523D362A1AABE504AE4731D97C03A4ED60C",
            "12f6b8eeec7e417f9b56ed3635aed523d362a1aabe504ae4731d97c03a4ed60c",
        ));
    }

    #[test]
    fn internal_to_external_address_normalizes_internal_forms() {
        assert_eq!(
            internal_to_external_address(
                "0:12f6b8eeec7e417f9b56ed3635aed523d362a1aabe504ae4731d97c03a4ed60c"
            ),
            ":12f6b8eeec7e417f9b56ed3635aed523d362a1aabe504ae4731d97c03a4ed60c"
        );
    }

    #[test]
    fn cursor_roundtrip() {
        let cursor = encode_cursor(100, 42);
        let (denom, order_id) = decode_cursor(&cursor).unwrap();
        assert_eq!(denom, 100);
        assert_eq!(order_id, 42);
    }

    #[test]
    fn decode_cursor_rejects_invalid_input() {
        use base64::Engine;
        assert!(decode_cursor("not-base64!!!").is_err());
        // Valid base64 but wrong format (no colon).
        let no_colon = base64::engine::general_purpose::STANDARD.encode("12345");
        assert!(decode_cursor(&no_colon).is_err());
    }
}
