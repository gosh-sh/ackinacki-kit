use std::fmt::Display;

use serde::Deserialize;

use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::event::Event;
use crate::traits::DecodeMessage;
use crate::traits::FromEvent;
use crate::KitResult;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u128)]
pub enum AuthServiceEvent {
    AuthProfileDeployed = 201,
}

impl TryFrom<String> for AuthServiceEvent {
    type Error = KitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let cleaned = value.replace(":", "");
        let number = u128::from_str_radix(&cleaned, 16).map_err(|e| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::Parse,
                format!("Parse authservice event `{cleaned}` into u128 ({e})"),
            )
        })?;

        match number {
            201 => Ok(AuthServiceEvent::AuthProfileDeployed),
            _ => Err(KitError::new(
                KitModule::Event,
                KitErrorCode::UnknownEvent,
                format!("Unknown authservice event `{cleaned}`"),
            )),
        }
    }
}

impl Display for AuthServiceEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{:064x}", *self as u128)
    }
}

impl AuthServiceEvent {
    pub fn to_address(&self) -> String {
        format!("0:{:064x}", *self as u128)
    }
}

pub enum DecodedAuthServiceEvent {
    AuthProfileDeployed { event: Event, kind: AuthServiceEvent, data: AuthProfileDeployedData },
}

impl FromEvent for DecodedAuthServiceEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let kind = AuthServiceEvent::try_from(event.dst.clone())?;

        match kind {
            AuthServiceEvent::AuthProfileDeployed => {
                let decoded = event.decode::<AuthProfileDeployedData>(contract)?;
                let data = decoded.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for authservice event `{}`", event.dst),
                    )
                })?;

                Ok(DecodedAuthServiceEvent::AuthProfileDeployed {
                    event: event.clone(),
                    kind,
                    data,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthProfileDeployedData {
    pub profile: String,
    #[serde(rename = "pubkeyHash")]
    pub pubkey_hash: String,
}
