use std::fmt::Display;

use serde::Deserialize;

use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;
use crate::deserialize::deserialize_u8;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::event::Event;
use crate::traits::DecodeMessage;
use crate::traits::FromEvent;
use crate::KitResult;

/// External event IDs are defined in `dex/modifiers/modifiers.sol`.
/// `OB_EPOCH_SETTLED = 145` exists as a constant but is no longer emitted
/// by the contract; intentionally omitted here.
///
/// `Extern0 = 0` is a synthetic catch-all for the five OrderBook events
/// (`PartialFill`, `FullyFilled`, `Queued`, `Rejected`, `CallbackBounced`)
/// that the contract emits to `address.makeAddrExtern(0, 256)` — i.e. they
/// all share `dst = 0:0…0` and cannot be disambiguated by destination
/// alone. `DecodedOrderBookEvent::from_event` decodes the body and routes
/// by ABI event name.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u128)]
/// External events emitted by `OrderBook`.
pub enum OrderBookEvent {
    Extern0 = 0,
    OrderPlaced = 143,
    OrderCancelled = 144,
    OrderFilled = 146,
}

impl TryFrom<String> for OrderBookEvent {
    type Error = KitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let cleaned = value.replace(":", "");
        let number = u128::from_str_radix(&cleaned, 16).map_err(|e| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::Parse,
                format!("Parse order book event `{cleaned}` into u128 ({e})"),
            )
        })?;

        match number {
            0 => Ok(OrderBookEvent::Extern0),
            143 => Ok(OrderBookEvent::OrderPlaced),
            144 => Ok(OrderBookEvent::OrderCancelled),
            146 => Ok(OrderBookEvent::OrderFilled),
            _ => Err(KitError::new(
                KitModule::Event,
                KitErrorCode::UnknownEvent,
                format!("Unknown order book event `{cleaned}`"),
            )),
        }
    }
}

impl Display for OrderBookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{:064x}", *self as u128)
    }
}

impl OrderBookEvent {
    pub fn to_address(&self) -> String {
        format!("0:{:064x}", *self as u128)
    }
}

/// Typed decoded `OrderBook` external event.
pub enum DecodedOrderBookEvent {
    OrderPlaced { event: Event, kind: OrderBookEvent, data: OrderPlacedData },
    OrderCancelled { event: Event, kind: OrderBookEvent, data: OrderCancelledData },
    OrderFilled { event: Event, kind: OrderBookEvent, data: OrderFilledData },
    PartialFill { event: Event, kind: OrderBookEvent, data: PartialFillData },
    FullyFilled { event: Event, kind: OrderBookEvent, data: FullyFilledData },
    Queued { event: Event, kind: OrderBookEvent, data: QueuedData },
    Rejected { event: Event, kind: OrderBookEvent, data: RejectedData },
    CallbackBounced { event: Event, kind: OrderBookEvent, data: CallbackBouncedData },
}

impl FromEvent for DecodedOrderBookEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let kind = OrderBookEvent::try_from(event.dst.clone())?;
        match kind {
            OrderBookEvent::OrderPlaced => {
                let data = decode_or_err::<OrderPlacedData>(event, contract)?;
                Ok(DecodedOrderBookEvent::OrderPlaced { event: event.clone(), kind, data })
            }
            OrderBookEvent::OrderCancelled => {
                let data = decode_or_err::<OrderCancelledData>(event, contract)?;
                Ok(DecodedOrderBookEvent::OrderCancelled { event: event.clone(), kind, data })
            }
            OrderBookEvent::OrderFilled => {
                let data = decode_or_err::<OrderFilledData>(event, contract)?;
                Ok(DecodedOrderBookEvent::OrderFilled { event: event.clone(), kind, data })
            }
            OrderBookEvent::Extern0 => {
                // Five events share the same extern address (id 0); disambiguate by
                // the ABI event name returned by `decode_message_body`.
                let decoded = contract.decode_message_body(&event.body)?;
                let name = decoded.name.as_str();
                let value = decoded.value.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!(
                            "Unexpected empty data for OrderBook extern-0 event `{name}` ({})",
                            event.dst
                        ),
                    )
                })?;
                match name {
                    "PartialFill" => {
                        let data = deserialize_from_value::<PartialFillData>(name, value)?;
                        Ok(DecodedOrderBookEvent::PartialFill { event: event.clone(), kind, data })
                    }
                    "FullyFilled" => {
                        let data = deserialize_from_value::<FullyFilledData>(name, value)?;
                        Ok(DecodedOrderBookEvent::FullyFilled { event: event.clone(), kind, data })
                    }
                    "Queued" => {
                        let data = deserialize_from_value::<QueuedData>(name, value)?;
                        Ok(DecodedOrderBookEvent::Queued { event: event.clone(), kind, data })
                    }
                    "Rejected" => {
                        let data = deserialize_from_value::<RejectedData>(name, value)?;
                        Ok(DecodedOrderBookEvent::Rejected { event: event.clone(), kind, data })
                    }
                    "CallbackBounced" => {
                        let data = deserialize_from_value::<CallbackBouncedData>(name, value)?;
                        Ok(DecodedOrderBookEvent::CallbackBounced {
                            event: event.clone(),
                            kind,
                            data,
                        })
                    }
                    _ => Err(KitError::new(
                        KitModule::Event,
                        KitErrorCode::UnknownEvent,
                        format!("Unknown OrderBook extern-0 event `{name}`"),
                    )),
                }
            }
        }
    }
}

fn decode_or_err<T>(event: &Event, contract: &impl DecodeMessage) -> KitResult<T>
where
    T: serde::de::DeserializeOwned,
{
    let decoded = event.decode::<T>(contract)?;
    decoded.ok_or_else(|| {
        KitError::new(
            KitModule::Event,
            KitErrorCode::EmptyData,
            format!("Unexpected empty data for order book event `{}`", event.dst),
        )
    })
}

fn deserialize_from_value<T>(name: &str, value: serde_json::Value) -> KitResult<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value::<T>(value).map_err(|e| {
        KitError::new(
            KitModule::Event,
            KitErrorCode::DeserializeFailed,
            format!("Deserialize OrderBook event `{name}` ({e})"),
        )
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `OrderBookEvent::OrderPlaced`.
pub struct OrderPlacedData {
    #[serde(deserialize_with = "deserialize_u128")]
    pub order_id: u128,
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
    pub client_order_id: u128,
    /// `uint256` represented as returned by ABI.
    pub deposit_hash: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub op_nonce: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `OrderBookEvent::OrderCancelled`.
pub struct OrderCancelledData {
    #[serde(deserialize_with = "deserialize_u128")]
    pub order_id: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub client_order_id: u128,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `OrderBookEvent::OrderFilled`.
pub struct OrderFilledData {
    #[serde(deserialize_with = "deserialize_u128")]
    pub order_id: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub filled_amount: u128,
    /// `uint256` represented as returned by ABI.
    pub clearing_price: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub fee_amount: u128,
    pub is_taker: bool,
    #[serde(deserialize_with = "deserialize_u64")]
    pub match_id: u64,
    /// `uint256` represented as returned by ABI.
    pub deposit_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `OrderBookEvent::PartialFill`. Emitted via the extern-0 channel
/// whenever a resting maker order absorbs a partial fill.
pub struct PartialFillData {
    #[serde(deserialize_with = "deserialize_u128")]
    pub order_id: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub client_order_id: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub filled_amount: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub remaining_amount: u128,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `OrderBookEvent::FullyFilled`. Emitted via the extern-0 channel
/// when an order's remaining amount reaches zero.
pub struct FullyFilledData {
    #[serde(deserialize_with = "deserialize_u128")]
    pub order_id: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub client_order_id: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub filled_amount: u128,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `OrderBookEvent::Queued`. Emitted when an op is admitted into
/// the matching queue.
pub struct QueuedData {
    #[serde(deserialize_with = "deserialize_u8")]
    pub slot: u8,
    #[serde(deserialize_with = "deserialize_u32")]
    pub queue_id: u32,
    #[serde(deserialize_with = "deserialize_u8")]
    pub entry_type: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `OrderBookEvent::Rejected`. Emitted when an op cannot be queued
/// (queue full / shutdown / invalid).
pub struct RejectedData {
    #[serde(deserialize_with = "deserialize_u8")]
    pub entry_type: u8,
    /// `uint256` represented as returned by ABI.
    pub deposit_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `OrderBookEvent::CallbackBounced`. Emitted when a callback to
/// a `PrivateNote` bounces back to the OrderBook.
pub struct CallbackBouncedData {
    pub dest: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub lt: u64,
}
