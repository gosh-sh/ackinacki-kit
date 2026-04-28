use std::fmt::Display;

use serde::Deserialize;

use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
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
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u128)]
/// External events emitted by `OrderBook`.
pub enum OrderBookEvent {
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
}

impl FromEvent for DecodedOrderBookEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let kind = OrderBookEvent::try_from(event.dst.clone())?;
        match kind {
            OrderBookEvent::OrderPlaced => {
                let decoded = event.decode::<OrderPlacedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for order book event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedOrderBookEvent::OrderPlaced { event: event.clone(), kind, data })
            }
            OrderBookEvent::OrderCancelled => {
                let decoded = event.decode::<OrderCancelledData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for order book event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedOrderBookEvent::OrderCancelled { event: event.clone(), kind, data })
            }
            OrderBookEvent::OrderFilled => {
                let decoded = event.decode::<OrderFilledData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for order book event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedOrderBookEvent::OrderFilled { event: event.clone(), kind, data })
            }
        }
    }
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
}
