use std::fmt::Display;

use serde::Deserialize;

use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u16;
use crate::deserialize::deserialize_u64;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::event::Event;
use crate::traits::DecodeMessage;
use crate::traits::FromEvent;
use crate::KitResult;

/// External event IDs are defined in `accumulator/modifiers/modifiers.sol`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u128)]
pub enum AccumulatorRootEvent {
    SellOrderCreated = 610,
    ShellPurchased = 611,
    UsdcClaimed = 612,
    NacklRedeemed = 613,
    MatchedOrders = 617,
}

// TODO(contracts/accumulator): modifiers.sol also defines external ids
// `UsdcMigratedEmit = 615` and `UsdcMintedEmit = 616`.
// They are not emitted by the current on-chain root implementation, so they
// are intentionally excluded from `AccumulatorRootEvent` for now.
// Add typed support here as soon as corresponding Solidity `emit` paths appear.

impl AccumulatorRootEvent {
    pub const ALL: [Self; 5] = [
        Self::SellOrderCreated,
        Self::ShellPurchased,
        Self::UsdcClaimed,
        Self::NacklRedeemed,
        Self::MatchedOrders,
    ];

    /// Returns external destination form used by GraphQL / query_collection.
    pub fn to_external_address(&self) -> String {
        format!(":{:064x}", *self as u128)
    }
}

impl TryFrom<String> for AccumulatorRootEvent {
    type Error = KitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let cleaned = value.replace(":", "");
        let number = u128::from_str_radix(&cleaned, 16).map_err(|e| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::Parse,
                format!("Parse accumulator root event `{cleaned}` into u128 ({e})"),
            )
        })?;

        match number {
            610 => Ok(Self::SellOrderCreated),
            611 => Ok(Self::ShellPurchased),
            612 => Ok(Self::UsdcClaimed),
            613 => Ok(Self::NacklRedeemed),
            617 => Ok(Self::MatchedOrders),
            _ => Err(KitError::new(
                KitModule::Event,
                KitErrorCode::UnknownEvent,
                format!("Unknown accumulator root event `{cleaned}`"),
            )),
        }
    }
}

impl Display for AccumulatorRootEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{:064x}", *self as u128)
    }
}

pub enum DecodedAccumulatorRootEvent {
    SellOrderCreated { event: Event, kind: AccumulatorRootEvent, data: SellOrderCreatedData },
    ShellPurchased { event: Event, kind: AccumulatorRootEvent, data: ShellPurchasedData },
    UsdcClaimed { event: Event, kind: AccumulatorRootEvent, data: UsdcClaimedData },
    NacklRedeemed { event: Event, kind: AccumulatorRootEvent, data: NacklRedeemedData },
    MatchedOrders { event: Event, kind: AccumulatorRootEvent, data: MatchedOrdersData },
}

impl DecodedAccumulatorRootEvent {
    pub fn event(&self) -> &Event {
        match self {
            Self::SellOrderCreated { event, .. } => event,
            Self::ShellPurchased { event, .. } => event,
            Self::UsdcClaimed { event, .. } => event,
            Self::NacklRedeemed { event, .. } => event,
            Self::MatchedOrders { event, .. } => event,
        }
    }
}

impl FromEvent for DecodedAccumulatorRootEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let kind = AccumulatorRootEvent::try_from(event.dst.clone())?;
        match kind {
            AccumulatorRootEvent::SellOrderCreated => {
                let decoded = event.decode::<SellOrderCreatedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for accumulator root event `{}`", event.dst),
                    )
                })?;
                Ok(Self::SellOrderCreated { event: event.clone(), kind, data })
            }
            AccumulatorRootEvent::ShellPurchased => {
                let decoded = event.decode::<ShellPurchasedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for accumulator root event `{}`", event.dst),
                    )
                })?;
                Ok(Self::ShellPurchased { event: event.clone(), kind, data })
            }
            AccumulatorRootEvent::UsdcClaimed => {
                let decoded = event.decode::<UsdcClaimedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for accumulator root event `{}`", event.dst),
                    )
                })?;
                Ok(Self::UsdcClaimed { event: event.clone(), kind, data })
            }
            AccumulatorRootEvent::NacklRedeemed => {
                let decoded = event.decode::<NacklRedeemedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for accumulator root event `{}`", event.dst),
                    )
                })?;
                Ok(Self::NacklRedeemed { event: event.clone(), kind, data })
            }
            AccumulatorRootEvent::MatchedOrders => {
                let decoded = event.decode::<MatchedOrdersData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for accumulator root event `{}`", event.dst),
                    )
                })?;
                Ok(Self::MatchedOrders { event: event.clone(), kind, data })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SellOrderCreatedData {
    pub seller: String,
    #[serde(deserialize_with = "deserialize_u16")]
    pub denom: u16,
    #[serde(rename = "orderId", deserialize_with = "deserialize_u64")]
    pub order_id: u64,
    #[serde(rename = "shellAmount", deserialize_with = "deserialize_u128")]
    pub shell_amount: u128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShellPurchasedData {
    pub buyer: String,
    #[serde(rename = "usdcAmount", deserialize_with = "deserialize_u128")]
    pub usdc_amount: u128,
    #[serde(rename = "shellFromSellers", deserialize_with = "deserialize_u128")]
    pub shell_from_sellers: u128,
    #[serde(rename = "shellMinted", deserialize_with = "deserialize_u128")]
    pub shell_minted: u128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsdcClaimedData {
    #[serde(rename = "orderId", deserialize_with = "deserialize_u64")]
    pub order_id: u64,
    #[serde(deserialize_with = "deserialize_u16")]
    pub denom: u16,
    pub seller: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub payout: u128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NacklRedeemedData {
    pub recipient: String,
    #[serde(rename = "burnAmount", deserialize_with = "deserialize_u128")]
    pub burn_amount: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub payout: u128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatchedOrdersData {
    #[serde(rename = "lastSold1", deserialize_with = "deserialize_u64")]
    pub last_sold_1: u64,
    #[serde(rename = "lastSold10", deserialize_with = "deserialize_u64")]
    pub last_sold_10: u64,
    #[serde(rename = "lastSold100", deserialize_with = "deserialize_u64")]
    pub last_sold_100: u64,
    #[serde(rename = "lastSold1000", deserialize_with = "deserialize_u64")]
    pub last_sold_1000: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SellOrderLotEvent {
    ClaimInitiated,
    OrderDestroyed,
}

impl TryFrom<&str> for SellOrderLotEvent {
    type Error = KitError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "ClaimInitiated" => Ok(Self::ClaimInitiated),
            "OrderDestroyed" => Ok(Self::OrderDestroyed),
            _ => Err(KitError::new(
                KitModule::Event,
                KitErrorCode::UnknownEvent,
                format!("Unknown sell-order-lot event `{value}`"),
            )),
        }
    }
}

impl Display for SellOrderLotEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClaimInitiated => write!(f, "ClaimInitiated"),
            Self::OrderDestroyed => write!(f, "OrderDestroyed"),
        }
    }
}

pub enum DecodedSellOrderLotEvent {
    ClaimInitiated { event: Event, kind: SellOrderLotEvent, data: ClaimInitiatedData },
    OrderDestroyed { event: Event, kind: SellOrderLotEvent, data: OrderDestroyedData },
}

impl DecodedSellOrderLotEvent {
    pub fn event(&self) -> &Event {
        match self {
            Self::ClaimInitiated { event, .. } => event,
            Self::OrderDestroyed { event, .. } => event,
        }
    }
}

impl FromEvent for DecodedSellOrderLotEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let decoded = contract.decode_message(event.boc.clone())?;
        let kind = SellOrderLotEvent::try_from(decoded.name.as_str())?;
        let raw = decoded.value.ok_or_else(|| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::EmptyData,
                format!("Unexpected empty data for sell-order-lot event `{}`", decoded.name),
            )
        })?;

        match kind {
            SellOrderLotEvent::ClaimInitiated => {
                let data = serde_json::from_value::<ClaimInitiatedData>(raw).map_err(|e| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::DeserializeFailed,
                        format!("Deserialize sell-order-lot event `{}` ({e})", decoded.name),
                    )
                })?;
                Ok(Self::ClaimInitiated { event: event.clone(), kind, data })
            }
            SellOrderLotEvent::OrderDestroyed => {
                let data = serde_json::from_value::<OrderDestroyedData>(raw).map_err(|e| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::DeserializeFailed,
                        format!("Deserialize sell-order-lot event `{}` ({e})", decoded.name),
                    )
                })?;
                Ok(Self::OrderDestroyed { event: event.clone(), kind, data })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaimInitiatedData {
    #[serde(rename = "orderId", deserialize_with = "deserialize_u64")]
    pub order_id: u64,
    #[serde(deserialize_with = "deserialize_u16")]
    pub denom: u16,
    pub owner: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderDestroyedData {
    #[serde(rename = "orderId", deserialize_with = "deserialize_u64")]
    pub order_id: u64,
    #[serde(deserialize_with = "deserialize_u16")]
    pub denom: u16,
    #[serde(deserialize_with = "deserialize_u128")]
    pub amount: u128,
}

#[cfg(test)]
mod tests {
    use super::AccumulatorRootEvent;

    #[test]
    fn root_event_address_roundtrip_accepts_colon_forms() {
        let dst = AccumulatorRootEvent::UsdcClaimed.to_external_address();
        let parsed = AccumulatorRootEvent::try_from(dst).expect("parse :... form");
        assert_eq!(parsed, AccumulatorRootEvent::UsdcClaimed);

        let dst_with_workchain_prefix = format!("0{}", AccumulatorRootEvent::NacklRedeemed);
        let parsed_prefixed =
            AccumulatorRootEvent::try_from(dst_with_workchain_prefix).expect("parse 0:... form");
        assert_eq!(parsed_prefixed, AccumulatorRootEvent::NacklRedeemed);

        let matched_orders_dst = AccumulatorRootEvent::MatchedOrders.to_external_address();
        let parsed_matched =
            AccumulatorRootEvent::try_from(matched_orders_dst).expect("parse MatchedOrders");
        assert_eq!(parsed_matched, AccumulatorRootEvent::MatchedOrders);
    }
}
