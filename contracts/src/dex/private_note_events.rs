use std::fmt::Display;

use serde::Deserialize;

use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u128_vec;
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
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u128)]
/// External events emitted by `PrivateNote`.
pub enum PrivateNoteEvent {
    PmpDeployed = 111,
    OwnerChanged = 112,
    StakeConfirmed = 113,
    ClaimAccepted = 114,
    StakeCancelled = 115,
    FullSetStakeConfirmed = 116,
    FullSetStakeCancelled = 117,
    TransferInitiated = 149,
    TransferReceived = 150,
}

impl TryFrom<String> for PrivateNoteEvent {
    type Error = KitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let cleaned = value.replace(":", "");
        let number = u128::from_str_radix(&cleaned, 16).map_err(|e| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::Parse,
                format!("Parse private note event `{cleaned}` into u128 ({e})"),
            )
        })?;

        match number {
            111 => Ok(PrivateNoteEvent::PmpDeployed),
            112 => Ok(PrivateNoteEvent::OwnerChanged),
            113 => Ok(PrivateNoteEvent::StakeConfirmed),
            114 => Ok(PrivateNoteEvent::ClaimAccepted),
            115 => Ok(PrivateNoteEvent::StakeCancelled),
            116 => Ok(PrivateNoteEvent::FullSetStakeConfirmed),
            117 => Ok(PrivateNoteEvent::FullSetStakeCancelled),
            149 => Ok(PrivateNoteEvent::TransferInitiated),
            150 => Ok(PrivateNoteEvent::TransferReceived),
            _ => Err(KitError::new(
                KitModule::Event,
                KitErrorCode::UnknownEvent,
                format!("Unknown private note event `{cleaned}`"),
            )),
        }
    }
}

impl Display for PrivateNoteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{:064x}", *self as u128)
    }
}

impl PrivateNoteEvent {
    pub fn to_address(&self) -> String {
        format!("0:{:064x}", *self as u128)
    }
}

/// Typed decoded `PrivateNote` external event.
pub enum DecodedPrivateNoteEvent {
    PmpDeployed { event: Event, kind: PrivateNoteEvent, data: PmpDeployedData },
    OwnerChanged { event: Event, kind: PrivateNoteEvent, data: OwnerChangedData },
    StakeConfirmed { event: Event, kind: PrivateNoteEvent, data: StakeConfirmedData },
    ClaimAccepted { event: Event, kind: PrivateNoteEvent, data: ClaimAcceptedData },
    StakeCancelled { event: Event, kind: PrivateNoteEvent, data: StakeCancelledData },
    FullSetStakeConfirmed { event: Event, kind: PrivateNoteEvent, data: FullSetStakeConfirmedData },
    FullSetStakeCancelled { event: Event, kind: PrivateNoteEvent, data: FullSetStakeCancelledData },
    TransferInitiated { event: Event, kind: PrivateNoteEvent, data: TransferInitiatedData },
    TransferReceived { event: Event, kind: PrivateNoteEvent, data: TransferReceivedData },
}

impl FromEvent for DecodedPrivateNoteEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let kind = PrivateNoteEvent::try_from(event.dst.clone())?;
        match kind {
            PrivateNoteEvent::PmpDeployed => {
                let decoded = event.decode::<PmpDeployedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::PmpDeployed { event: event.clone(), kind, data })
            }
            PrivateNoteEvent::OwnerChanged => {
                let decoded = event.decode::<OwnerChangedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::OwnerChanged { event: event.clone(), kind, data })
            }
            PrivateNoteEvent::StakeConfirmed => {
                let decoded = event.decode::<StakeConfirmedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::StakeConfirmed { event: event.clone(), kind, data })
            }
            PrivateNoteEvent::ClaimAccepted => {
                let decoded = event.decode::<ClaimAcceptedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::ClaimAccepted { event: event.clone(), kind, data })
            }
            PrivateNoteEvent::StakeCancelled => {
                let decoded = event.decode::<StakeCancelledData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::StakeCancelled { event: event.clone(), kind, data })
            }
            PrivateNoteEvent::FullSetStakeConfirmed => {
                let decoded = event.decode::<FullSetStakeConfirmedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::FullSetStakeConfirmed {
                    event: event.clone(),
                    kind,
                    data,
                })
            }
            PrivateNoteEvent::FullSetStakeCancelled => {
                let decoded = event.decode::<FullSetStakeCancelledData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::FullSetStakeCancelled {
                    event: event.clone(),
                    kind,
                    data,
                })
            }
            PrivateNoteEvent::TransferInitiated => {
                let decoded = event.decode::<TransferInitiatedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::TransferInitiated { event: event.clone(), kind, data })
            }
            PrivateNoteEvent::TransferReceived => {
                let decoded = event.decode::<TransferReceivedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for private note event `{}`", event.dst),
                    )
                })?;
                Ok(DecodedPrivateNoteEvent::TransferReceived { event: event.clone(), kind, data })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::OwnerChanged`.
pub struct OwnerChangedData {
    #[serde(rename = "oldPubkey")]
    pub old_pubkey: String,
    #[serde(rename = "newPubkey")]
    pub new_pubkey: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::StakeConfirmed`.
pub struct StakeConfirmedData {
    #[serde(rename = "stakeController")]
    pub stake_controller: String,
    #[serde(deserialize_with = "deserialize_u32")]
    pub outcome: u32,
    #[serde(deserialize_with = "deserialize_u128")]
    pub amount: u128,
    #[serde(rename = "betType", deserialize_with = "deserialize_u8")]
    pub bet_type: u8,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::StakeCancelled`.
pub struct StakeCancelledData {
    #[serde(rename = "stakeController")]
    pub stake_controller: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub value: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::FullSetStakeConfirmed`.
pub struct FullSetStakeConfirmedData {
    #[serde(rename = "stakeController")]
    pub stake_controller: String,
    #[serde(deserialize_with = "deserialize_u128_vec")]
    pub amount: Vec<u128>,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::FullSetStakeCancelled`.
pub struct FullSetStakeCancelledData {
    #[serde(rename = "stakeController")]
    pub stake_controller: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub value: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::ClaimAccepted`.
pub struct ClaimAcceptedData {
    #[serde(rename = "stakeController")]
    pub stake_controller: String,
    pub outcome: Option<String>,
    #[serde(deserialize_with = "deserialize_u128")]
    pub payout: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::PmpDeployed`.
pub struct PmpDeployedData {
    #[serde(rename = "eventId")]
    pub event_id: String,
    #[serde(rename = "tokenType", deserialize_with = "deserialize_u32")]
    pub token_type: u32,
    #[serde(rename = "pmpAddress")]
    pub pmp_address: String,
    #[serde(rename = "oracleEventLists")]
    pub oracle_event_lists: Vec<String>,
    #[serde(rename = "oracleFee", deserialize_with = "deserialize_u128_vec")]
    pub oracle_fee: Vec<u128>,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::TransferInitiated`.
pub struct TransferInitiatedData {
    pub dest: String,
    #[serde(rename = "tokenType", deserialize_with = "deserialize_u32")]
    pub token_type: u32,
    #[serde(deserialize_with = "deserialize_u128")]
    pub amount: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Payload of `PrivateNoteEvent::TransferReceived`.
pub struct TransferReceivedData {
    pub from: String,
    #[serde(rename = "tokenType", deserialize_with = "deserialize_u32")]
    pub token_type: u32,
    #[serde(deserialize_with = "deserialize_u128")]
    pub amount: u128,
}
