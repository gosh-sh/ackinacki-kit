use std::fmt::Display;

use serde::Deserialize;

use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::event::Event;
use crate::traits::DecodeMessage;
use crate::traits::FromEvent;
use crate::KitResult;

/// External event IDs are defined in `exchange/modifiers/modifiers.sol`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u128)]
pub enum UsdcBridgeEvent {
    UsdcMigrated = 615,
    UsdcMinted = 616,
    WithdrawalInitiated = 618,
    DepositFinalized = 619,
}

impl UsdcBridgeEvent {
    /// Returns external destination form used by GraphQL / query_collection.
    pub fn to_external_address(&self) -> String {
        format!(":{:064x}", *self as u128)
    }
}

impl TryFrom<String> for UsdcBridgeEvent {
    type Error = KitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let cleaned = value.replace(":", "");
        let number = u128::from_str_radix(&cleaned, 16).map_err(|e| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::Parse,
                format!("Parse usdc bridge event `{cleaned}` into u128 ({e})"),
            )
        })?;

        match number {
            615 => Ok(Self::UsdcMigrated),
            616 => Ok(Self::UsdcMinted),
            618 => Ok(Self::WithdrawalInitiated),
            619 => Ok(Self::DepositFinalized),
            _ => Err(KitError::new(
                KitModule::Event,
                KitErrorCode::UnknownEvent,
                format!("Unknown usdc bridge event `{cleaned}`"),
            )),
        }
    }
}

impl Display for UsdcBridgeEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{:064x}", *self as u128)
    }
}

#[derive(Debug)]
pub enum DecodedUsdcBridgeEvent {
    UsdcMigrated { event: Event, kind: UsdcBridgeEvent, data: UsdcMigratedData },
    UsdcMinted { event: Event, kind: UsdcBridgeEvent, data: UsdcMintedData },
    WithdrawalInitiated { event: Event, kind: UsdcBridgeEvent, data: WithdrawalInitiatedData },
    DepositFinalized { event: Event, kind: UsdcBridgeEvent, data: DepositFinalizedData },
}

impl DecodedUsdcBridgeEvent {
    pub fn event(&self) -> &Event {
        match self {
            Self::UsdcMigrated { event, .. } => event,
            Self::UsdcMinted { event, .. } => event,
            Self::WithdrawalInitiated { event, .. } => event,
            Self::DepositFinalized { event, .. } => event,
        }
    }
}

impl FromEvent for DecodedUsdcBridgeEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let kind = UsdcBridgeEvent::try_from(event.dst.clone())?;
        match kind {
            UsdcBridgeEvent::UsdcMigrated => {
                let decoded = event.decode::<UsdcMigratedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for usdc bridge event `{}`", event.dst),
                    )
                })?;
                Ok(Self::UsdcMigrated { event: event.clone(), kind, data })
            }
            UsdcBridgeEvent::UsdcMinted => {
                let decoded = event.decode::<UsdcMintedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for usdc bridge event `{}`", event.dst),
                    )
                })?;
                Ok(Self::UsdcMinted { event: event.clone(), kind, data })
            }
            UsdcBridgeEvent::WithdrawalInitiated => {
                let decoded = event.decode::<WithdrawalInitiatedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for usdc bridge event `{}`", event.dst),
                    )
                })?;
                Ok(Self::WithdrawalInitiated { event: event.clone(), kind, data })
            }
            UsdcBridgeEvent::DepositFinalized => {
                let decoded = event.decode::<DepositFinalizedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for usdc bridge event `{}`", event.dst),
                    )
                })?;
                Ok(Self::DepositFinalized { event: event.clone(), kind, data })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `UsdcBridgeEvent::UsdcMigrated`.
pub struct UsdcMigratedData {
    pub from: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub value: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `UsdcBridgeEvent::UsdcMinted`.
pub struct UsdcMintedData {
    pub recipient: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub value: u128,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `UsdcBridgeEvent::WithdrawalInitiated`.
pub struct WithdrawalInitiatedData {
    /// `uint256` represented as returned by ABI.
    pub dst_chain_id: String,
    /// `bytes` payload encoded as hex.
    pub recipient: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub amount: u128,
    #[serde(deserialize_with = "deserialize_u32")]
    pub token_id: u32,
    pub sender: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Payload of `UsdcBridgeEvent::DepositFinalized`.
pub struct DepositFinalizedData {
    /// `uint256` represented as returned by ABI.
    pub src_dapp_id: String,
    /// `bytes` payload encoded as hex.
    pub src_sender: String,
    /// `uint256` represented as returned by ABI.
    pub recipient: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub amount: u128,
    #[serde(deserialize_with = "deserialize_u32")]
    pub token_id: u32,
    /// `uint256` represented as returned by ABI.
    pub src_deposit_id: String,
}
