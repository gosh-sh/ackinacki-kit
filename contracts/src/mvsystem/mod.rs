use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;

use crate::deserialize::deserialize_option_u16;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;

pub mod boost;
pub mod game_root;
pub mod indexer;
pub mod miner;
pub mod mirror;
pub mod multifactor;
pub mod popcoin_root;
pub mod popcoin_wallet;
pub mod popitgame;
pub mod root;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(u8)]
#[serde(from = "u8", into = "u8")]
pub enum ContractIndex {
    PopitGame = 1,
    PopCoinWallet = 2,
    PopCoinRoot = 4,
    MvMultifactor = 5,
    Indexer = 6,
    Boost = 7,
    Miner = 8,
    Mirror = 9,
}

impl From<u8> for ContractIndex {
    fn from(value: u8) -> Self {
        match value {
            1 => ContractIndex::PopitGame,
            2 => ContractIndex::PopCoinWallet,
            4 => ContractIndex::PopCoinRoot,
            5 => ContractIndex::MvMultifactor,
            6 => ContractIndex::Indexer,
            7 => ContractIndex::Boost,
            8 => ContractIndex::Miner,
            9 => ContractIndex::Mirror,
            _ => panic!("Unknown allowed payload destination {value}"),
        }
    }
}

impl From<ContractIndex> for u8 {
    fn from(value: ContractIndex) -> Self {
        value as u8
    }
}
