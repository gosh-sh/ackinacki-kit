use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;

use crate::deserialize::deserialize_option_u16;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;

pub mod boost;
pub mod indexer;
pub mod mirror;
pub mod mobile_verifiers_root;
pub mod mvmultifactor;
pub mod popcoin_root;
pub mod popcoin_wallet;
pub mod popitgame;

#[derive(Debug, Serialize, Deserialize)]
pub struct Popit {
    #[serde(deserialize_with = "deserialize_u128")]
    pub rewards: u128,
    #[serde(deserialize_with = "deserialize_u64")]
    pub value: u64,
    #[serde(rename = "leftRewards", deserialize_with = "deserialize_u128")]
    pub rewards_left: u128,
    #[serde(rename = "leftTaps", deserialize_with = "deserialize_u128")]
    pub taps_left: u128,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PopitMedia {
    pub id: String,
    #[serde(rename = "media")]
    pub file_id: String,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub protopopit: Option<u16>,
}

impl PopitMedia {
    pub fn new(file_id: impl AsRef<str>, id: Option<String>, proto_id: Option<u16>) -> Self {
        let id = match id {
            Some(value) => value.clone(),
            None => {
                let hash_data = format!("{}_{}", Utc::now().timestamp_millis(), file_id.as_ref());

                let mut hasher = sha2::Sha256::new();
                hasher.update(hash_data.as_bytes());
                format!("0x{}", hex::encode(hasher.finalize()))
            }
        };

        PopitMedia { id, file_id: file_id.as_ref().to_string(), protopopit: proto_id }
    }
}

#[derive(Debug, Deserialize)]
pub struct PopitCandidateWithMedia {
    #[serde(rename = "media")]
    pub file_id: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub value: u64,
    #[serde(deserialize_with = "deserialize_option_u16")]
    pub protopopit: Option<u16>,
    #[serde(deserialize_with = "deserialize_u32")]
    pub time: u32,
}
