use serde::Deserialize;

use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u16;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;
use crate::deserialize::deserialize_u8;

pub mod bk_wallet;
pub mod bm_wallet;
pub mod reputation;

#[derive(Debug, Clone, Deserialize)]
pub struct Stake {
    pub stake: String,
    #[serde(rename = "seqNoStart", deserialize_with = "deserialize_u64")]
    pub seqno_start: u64,
    #[serde(rename = "seqNoFinish", deserialize_with = "deserialize_u64")]
    pub seqno_finish: u64,
    pub bls_key: String,
    #[serde(deserialize_with = "deserialize_u8")]
    pub status: u8,
    #[serde(rename = "signerIndex", deserialize_with = "deserialize_u16")]
    pub signer_index: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LicenseData {
    #[serde(rename = "reputationTime", deserialize_with = "deserialize_u128")]
    pub reputation_time: u128,
    #[serde(deserialize_with = "deserialize_u8")]
    pub status: u8,
    #[serde(rename = "isPrivileged")]
    pub is_privileged: bool,
    #[serde(rename = "stakeController")]
    pub stake_controller: Option<String>,
    #[serde(deserialize_with = "deserialize_u64")]
    pub last_touch: u64,
    #[serde(deserialize_with = "deserialize_u128")]
    pub balance: u128,
    #[serde(rename = "lockStake", deserialize_with = "deserialize_u128")]
    pub lock_stake: u128,
    #[serde(rename = "lockContinue", deserialize_with = "deserialize_u128")]
    pub lock_continue: u128,
    #[serde(rename = "lockCooler", deserialize_with = "deserialize_u128")]
    pub lock_cooler: u128,
    #[serde(rename = "isLockToStake")]
    pub is_lock_to_stake: bool,
    #[serde(rename = "coolerCount", deserialize_with = "deserialize_u32")]
    pub cooler_count: u32,
    #[serde(rename = "isLockToStakeByWallet")]
    pub is_lock_to_stake_by_wallet: bool,
    #[serde(rename = "isLockBecauseOfSlashing")]
    pub is_lock_because_of_slashing: bool,
}
