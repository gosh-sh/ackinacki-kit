use std::fmt::Display;

use serde::Deserialize;

use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::event::Event;
use crate::traits::DecodeMessage;
use crate::traits::FromEvent;
use crate::KitResult;

/// External event IDs are defined in `dex/modifiers/modifiers.sol`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u128)]
/// External events emitted by `RootPN`.
pub enum RootPnEvent {
    PrivateNoteDeployed = 101,
    NullifierDeployed = 102,
    VoucherGenerated = 135,
}

impl TryFrom<String> for RootPnEvent {
    type Error = KitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let cleaned = value.replace(":", "");
        let number = u128::from_str_radix(&cleaned, 16).map_err(|e| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::Parse,
                format!("Parse root pn event `{cleaned}` into u128 ({e})"),
            )
        })?;

        match number {
            101 => Ok(RootPnEvent::PrivateNoteDeployed),
            102 => Ok(RootPnEvent::NullifierDeployed),
            135 => Ok(RootPnEvent::VoucherGenerated),
            _ => Err(KitError::new(
                KitModule::Event,
                KitErrorCode::UnknownEvent,
                format!("Unknown root pn event `{cleaned}`"),
            )),
        }
    }
}

impl Display for RootPnEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{:064x}", *self as u128)
    }
}

impl RootPnEvent {
    pub fn to_address(&self) -> String {
        format!("0:{:064x}", *self as u128)
    }
}

/// Typed decoded `RootPN` external event.
pub enum DecodedRootPnEvent {
    PrivateNoteDeployed { event: Event, kind: RootPnEvent, data: PrivateNoteDeployedData },
    NullifierDeployed { event: Event, kind: RootPnEvent, data: NullifierDeployedData },
    VoucherGenerated { event: Event, kind: RootPnEvent, data: VoucherGeneratedData },
}

impl FromEvent for DecodedRootPnEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let kind = RootPnEvent::try_from(event.dst.clone())?;

        match kind {
            RootPnEvent::PrivateNoteDeployed => {
                let decoded = event.decode::<PrivateNoteDeployedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for root pn event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedRootPnEvent::PrivateNoteDeployed { event: event.clone(), kind, data })
            }
            RootPnEvent::NullifierDeployed => {
                let decoded = event.decode::<NullifierDeployedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for root pn event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedRootPnEvent::NullifierDeployed { event: event.clone(), kind, data })
            }
            RootPnEvent::VoucherGenerated => {
                let decoded = event.decode::<VoucherGeneratedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for root pn event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedRootPnEvent::VoucherGenerated { event: event.clone(), kind, data })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `RootPnEvent::PrivateNoteDeployed`.
pub struct PrivateNoteDeployedData {
    #[serde(rename = "depositIdentifierHash")]
    pub deposit_identifier_hash: String,
    #[serde(rename = "noteAddress")]
    pub note_address: String,
    #[serde(rename = "initialBalance", deserialize_with = "deserialize_u128")]
    pub initial_balance: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `RootPnEvent::NullifierDeployed`.
pub struct NullifierDeployedData {
    #[serde(rename = "nullifierAddress")]
    pub nullifier_address: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub value: u64,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `RootPnEvent::VoucherGenerated`.
pub struct VoucherGeneratedData {
    #[serde(rename = "sk_u_commit")]
    pub sk_u_commit: String,
    #[serde(rename = "voucher_nominal")]
    pub voucher_nominal: String,
    #[serde(rename = "token_type", deserialize_with = "deserialize_u32")]
    pub token_type: u32,
}
