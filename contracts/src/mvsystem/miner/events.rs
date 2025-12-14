use std::fmt::Debug;
use std::fmt::Display;

use anyhow::anyhow;
use serde::Deserialize;

use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;
use crate::event::Event;
use crate::mvsystem::miner::SessionInterval;
use crate::traits::DecodeMessage;
use crate::traits::FromEvent;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u128)]
pub enum MinerEvent {
    SessionInterval = 5,
    SeedUpdated = 6,
    ComplexityUpdated = 7,
}

impl TryFrom<String> for MinerEvent {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let cleaned = value.replace(":", "");
        let number = u128::from_str_radix(&cleaned, 16)
            .map_err(|e| anyhow!("Parse miner event `{cleaned}` into u128 ({e})"))?;
        let event = match number {
            5 => MinerEvent::SessionInterval,
            6 => MinerEvent::SeedUpdated,
            7 => MinerEvent::ComplexityUpdated,
            _ => anyhow::bail!("Unknown miner event `{cleaned}`"),
        };
        Ok(event)
    }
}

impl Display for MinerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ":{}", hex::encode((*self as u128).to_be_bytes()))
    }
}

pub enum DecodedMinerEvent {
    SessionInterval { event: Event, kind: MinerEvent, data: SessionIntervalData },
    SeedUpdated { event: Event, kind: MinerEvent, data: SeedUpdatedData },
    ComplexityUpdated { event: Event, kind: MinerEvent, data: ComplexityUpdatedData },
}

impl FromEvent for DecodedMinerEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> anyhow::Result<Self> {
        let kind = MinerEvent::try_from(event.dst.clone())?;
        match kind {
            MinerEvent::SessionInterval => {
                let decoded = event
                    .decode::<SessionIntervalData>(contract)
                    .map_err(|e| anyhow!("Decode miner event `{}` ({e})", event.dst))?;
                let data = decoded.ok_or_else(|| {
                    anyhow!("Unexpected empty data for miner event `{}`", event.dst)
                })?;
                Ok(DecodedMinerEvent::SessionInterval { event: event.clone(), kind, data })
            }
            MinerEvent::SeedUpdated => {
                let decoded = event
                    .decode::<SeedUpdatedData>(contract)
                    .map_err(|e| anyhow!("Decode miner event `{}` ({e})", event.dst))?;
                let data = decoded.ok_or_else(|| {
                    anyhow!("Unexpected empty data for miner event `{}`", event.dst)
                })?;
                Ok(DecodedMinerEvent::SeedUpdated { event: event.clone(), kind, data })
            }
            MinerEvent::ComplexityUpdated => {
                let decoded = event
                    .decode::<ComplexityUpdatedData>(contract)
                    .map_err(|e| anyhow!("Decode miner event `{}` ({e})", event.dst))?;
                let data = decoded.ok_or_else(|| {
                    anyhow!("Unexpected empty data for miner event `{}`", event.dst)
                })?;
                Ok(DecodedMinerEvent::ComplexityUpdated { event: event.clone(), kind, data })
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SeedUpdatedData {
    pub seed: String,

    #[serde(rename = "seednext")]
    pub next_seed: String,
}

#[derive(Debug, Deserialize)]
pub struct ComplexityUpdatedData {
    #[serde(rename = "easyComplexity", deserialize_with = "deserialize_u32")]
    pub easy: u32,

    #[serde(rename = "hardComplexity", deserialize_with = "deserialize_u32")]
    pub hard: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionIntervalData {
    pub easy: SessionInterval,
    pub hard: SessionInterval,

    #[serde(rename = "workerId", deserialize_with = "deserialize_u64")]
    pub worker_id: u64,
}
