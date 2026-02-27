use std::fmt::Display;

use num_bigint::BigUint;
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
    /// Builds the external event destination address used by
    /// `AuthServiceRoot.onProfileDeployed` for a given `multifactorHash`.
    ///
    /// Solidity emits `AuthProfileDeployed` to
    /// `address.makeAddrExtern(multifactorHash, 256)`.
    /// In GraphQL message queries this address is represented as `:{hex}`.
    pub fn auth_profile_deployed_external_address(
        multifactor_hash: impl AsRef<str>,
    ) -> KitResult<String> {
        let value = parse_u256_str(multifactor_hash.as_ref())?;
        Ok(format!(":{value:064x}"))
    }
}

pub enum DecodedAuthServiceEvent {
    AuthProfileDeployed { event: Event, kind: AuthServiceEvent, data: AuthProfileDeployedData },
}

impl FromEvent for DecodedAuthServiceEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self> {
        let decoded = contract.decode_message(event.boc.clone())?;
        let kind = match decoded.name.as_str() {
            "AuthProfileDeployed" => AuthServiceEvent::AuthProfileDeployed,
            other => {
                return Err(KitError::new(
                    KitModule::Event,
                    KitErrorCode::UnknownEvent,
                    format!("Unknown authservice event name `{other}`"),
                ))
            }
        };

        match kind {
            AuthServiceEvent::AuthProfileDeployed => {
                let raw = decoded.value.ok_or_else(|| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::EmptyData,
                        format!("Unexpected empty data for authservice event `{}`", decoded.name),
                    )
                })?;
                let data = serde_json::from_value::<AuthProfileDeployedData>(raw).map_err(|e| {
                    KitError::new(
                        KitModule::Event,
                        KitErrorCode::DeserializeFailed,
                        format!("Deserialize authservice event `{}` ({e})", decoded.name),
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

fn parse_u256_str(value: &str) -> KitResult<BigUint> {
    let stripped = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")).unwrap_or(value);
    let is_hex = value.starts_with("0x") || value.starts_with("0X");
    let radix = if is_hex { 16 } else { 10 };
    BigUint::parse_bytes(stripped.as_bytes(), radix).ok_or_else(|| {
        KitError::new(
            KitModule::Event,
            KitErrorCode::Parse,
            format!("Parse uint256 string `{value}`"),
        )
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthProfileDeployedData {
    pub profile: String,
    #[serde(rename = "multifactorHash")]
    pub multifactor_hash: String,
    pub description: String,
}
