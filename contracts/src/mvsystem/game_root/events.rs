use std::fmt::Debug;

use anyhow::anyhow;
use serde::Deserialize;

use crate::deserialize::deserialize_u128;
use crate::event::Event;
use crate::traits::DecodeMessage;
use crate::traits::FromEvent;

pub enum DecodedMobileVerifiersGameRootEvent {
    RewardedPopitGame { event: Event, data: RewardedPopitGameData },
}

impl FromEvent for DecodedMobileVerifiersGameRootEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> anyhow::Result<Self> {
        let decoded = event
            .decode::<RewardedPopitGameData>(contract)
            .map_err(|e| anyhow!("Decode game root event `{}` ({e})", event.dst))?;
        let data = decoded
            .ok_or_else(|| anyhow!("Unexpected empty data for game root event `{}`", event.dst))?;

        Ok(DecodedMobileVerifiersGameRootEvent::RewardedPopitGame { event: event.clone(), data })
    }
}

#[derive(Debug, Deserialize)]
pub struct RewardedPopitGameData {
    #[serde(deserialize_with = "deserialize_u128")]
    pub reward: u128,
}
