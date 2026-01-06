use serde::Deserialize;

use crate::deserialize::deserialize_u64;

pub mod contract;
pub mod events;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SessionInterval {
    #[serde(rename = "first", deserialize_with = "deserialize_u64")]
    pub start: u64,

    #[serde(rename = "second", deserialize_with = "deserialize_u64")]
    pub end: u64,
}
