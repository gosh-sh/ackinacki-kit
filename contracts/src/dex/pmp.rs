use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_option_u32;
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u32_u8_u128_nested_map;
use crate::deserialize::deserialize_u64;
use crate::deserialize::deserialize_u128;
use crate::error::DexModule;
use crate::error::KitModule;
use crate::traits::AutoContract;
use crate::traits::AccountAccessor;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/dex/PMP.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for the DEX `PMP` (prediction market pool) contract.
pub struct Pmp {
    base: ContractBase,
}

impl ModuleAccessor for Pmp {
    const MODULE: KitModule = KitModule::Dex(DexModule::Pmp);
}

impl HasContractBase for Pmp {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for Pmp {}

impl AsyncGuarded<Account> for Pmp {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Pmp {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PMP.submitSetTimings`.
pub struct ParamsOfSubmitSetTimings {
    #[serde(rename(serialize = "stakeStart"))]
    pub stake_start: u64,
    #[serde(rename(serialize = "stakeEnd"))]
    pub stake_end: u64,
    #[serde(rename(serialize = "resultStart"))]
    pub result_start: u64,
    #[serde(rename(serialize = "resultEnd"))]
    pub result_end: u64,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PMP.submitResolve`.
pub struct ParamsOfSubmitResolve {
    #[serde(rename(serialize = "outcomeId"))]
    pub outcome_id: u32,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PMP.getDetails`.
///
/// `uint256` identity-like values are preserved as strings to avoid losing the
/// original ABI representation (decimal vs hex) and to avoid artificial size
/// limits in the wrapper API.
pub struct ResultOfGetDetails {
    /// Human-readable pool name.
    pub name: String,
    /// Event token/currency type.
    #[serde(rename = "token_type", deserialize_with = "deserialize_u32")]
    pub token_type: u32,
    /// Event identifier (`uint256`) as returned by ABI.
    pub event_id: String,
    /// Oracle list hash (`uint256`) as returned by ABI.
    pub oracle_list_hash: String,
    /// Deployer `PrivateNote` address.
    pub deployer: String,
    #[serde(rename = "privateNoteCodeHash")]
    pub private_note_code_hash: String,
    /// Total clean pool amount (without coupon accounting nuances handled by contract internals).
    #[serde(rename = "totalPool", deserialize_with = "deserialize_u128")]
    pub total_pool: u128,
    /// Whether staking timings were accepted and the pool is approved.
    pub approved: bool,
    #[serde(rename = "numOutcomes", deserialize_with = "deserialize_u32")]
    pub num_outcomes: u32,
    /// Final outcome if resolved.
    #[serde(rename = "resolvedOutcome", deserialize_with = "deserialize_option_u32")]
    pub resolved_outcome: Option<u32>,
    #[serde(rename = "stakeStart", deserialize_with = "deserialize_u64")]
    pub stake_start: u64,
    #[serde(rename = "stakeEnd", deserialize_with = "deserialize_u64")]
    pub stake_end: u64,
    #[serde(rename = "resultStart", deserialize_with = "deserialize_u64")]
    pub result_start: u64,
    #[serde(rename = "resultEnd", deserialize_with = "deserialize_u64")]
    pub result_end: u64,
    /// Whether oracle governance cancelled the event.
    #[serde(rename = "isCancelled")]
    pub is_cancelled: bool,
    /// Number of oracle confirmations required by the pool.
    #[serde(rename = "numberOfOracleEvents", deserialize_with = "deserialize_u128")]
    pub number_of_oracle_events: u128,
    /// Number of oracle confirmations currently collected.
    #[serde(rename = "approvedOracleEvents", deserialize_with = "deserialize_u128")]
    pub approved_oracle_events: u128,
    /// Nested mapping: `outcome_id -> bet_type -> pool_amount`.
    #[serde(rename = "typedOutcomePools")]
    #[serde(deserialize_with = "deserialize_u32_u8_u128_nested_map")]
    pub typed_outcome_pools: HashMap<u32, HashMap<u8, u128>>,
    /// Mapping of `outcome_id -> human-readable outcome name`.
    #[serde(rename = "outcomeNames")]
    pub outcome_names: HashMap<u32, String>,
    /// Creator fee accumulated by the pool.
    #[serde(rename = "creatorFee", deserialize_with = "deserialize_u128")]
    pub creator_fee: u128,
}

impl Pmp {
    /// Create a wrapper for a deployed `PMP`.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// # Submit timings vote (oracle governance)
    ///
    /// Original contract method: `submitSetTimings`
    ///
    /// Should be signed with an oracle key (or sent from a trusted internal
    /// oracle address bound in `approveEvent`).
    pub async fn submit_set_timings(
        &self,
        params: ParamsOfSubmitSetTimings,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "submitSetTimings".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Submit resolve vote (oracle governance)
    ///
    /// Original contract method: `submitResolve`
    pub async fn submit_resolve(
        &self,
        params: ParamsOfSubmitResolve,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "submitResolve".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Submit cancel-event vote (oracle governance)
    ///
    /// Original contract method: `submitCancelEvent`
    pub async fn submit_cancel_event(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set =
            CallSet { function_name: "submitCancelEvent".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Get PMP details
    ///
    /// Original contract method: `getDetails`
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    // Callback/internal flows (`approveEvent`, `acceptStake`, `cancelStake`,
    // `acceptFullSetStake`, `withdrawFullSet`, `claim`, `rejectEvent`) are
    // intentionally omitted here because Solidity restricts them by sender.
}
