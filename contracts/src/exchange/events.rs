use std::fmt::Display;

use serde::Deserialize;

use crate::deserialize::deserialize_u128;
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
pub enum UsdcExchangeEvent {
    UsdcMinted = 616,
}

impl UsdcExchangeEvent {
    /// Returns external destination form used by GraphQL / query_collection.
    pub fn to_external_address(&self) -> String {
        format!(":{:064x}", *self as u128)
    }
}

impl TryFrom<String> for UsdcExchangeEvent {
    type Error = KitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let cleaned = value.replace(":", "");
        let number = u128::from_str_radix(&cleaned, 16).map_err(|e| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::Parse,
                format!("Parse usdc exchange event `{cleaned}` into u128 ({e})"),
            )
        })?;

        match number {
            616 => Ok(Self::UsdcMinted),
            _ => Err(KitError::new(
                KitModule::Event,
                KitErrorCode::UnknownEvent,
                format!("Unknown usdc exchange event `{cleaned}`"),
            )),
        }
    }
}

impl Display for UsdcExchangeEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{:064x}", *self as u128)
    }
}

#[derive(Debug)]
pub enum DecodedUsdcExchangeEvent {
    UsdcMinted { event: Event, kind: UsdcExchangeEvent, data: UsdcMintedData },
}

impl DecodedUsdcExchangeEvent {
    pub fn event(&self) -> &Event {
        match self {
            Self::UsdcMinted { event, .. } => event,
        }
    }
}

impl FromEvent for DecodedUsdcExchangeEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let kind = UsdcExchangeEvent::try_from(event.dst.clone())?;
        match kind {
            UsdcExchangeEvent::UsdcMinted => {
                let decoded = event.decode::<UsdcMintedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for usdc exchange event `{}`", event.dst),
                    )
                })?;
                Ok(Self::UsdcMinted { event: event.clone(), kind, data })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsdcMintedData {
    pub recipient: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub value: u128,
}
