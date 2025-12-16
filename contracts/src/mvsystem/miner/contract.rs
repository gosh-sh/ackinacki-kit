use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_option_u64;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u128_vec;
use crate::deserialize::deserialize_u64;
use crate::deserialize::deserialize_u64_vec;
use crate::mvsystem::miner::SessionInterval;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../../abi/mvsystem/Miner.abi.json");

#[derive(Debug, Clone)]
pub struct Miner {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for Miner {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Miner {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Miner {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for Miner {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for Miner {}

impl DecodeMessage for Miner {}

impl Executor for Miner {}

impl SendMessage for Miner {}

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
impl AsyncGuarded<Account> for Miner {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T + 'async_trait,
        T: 'async_trait,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
impl AsyncGuardedMut<Account> for Miner {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut + 'async_trait,
        Fut: Future<Output = anyhow::Result<T>> + 'async_trait,
        T: 'async_trait,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetDetails {
    #[serde(rename = "mobileVerifiersContractGameRoot")]
    pub mobile_verifiers_game_root_address: String,

    #[serde(rename = "owner")]
    pub owner_address: String,

    #[serde(rename = "popitGame")]
    pub popitgame_address: String,

    #[serde(rename = "boost")]
    pub boost_address: String,

    #[serde(rename = "mbiCur", deserialize_with = "deserialize_option_u64")]
    pub mbi_cur: Option<u64>,

    #[serde(rename = "owner_pubkey")]
    pub owner_public: Option<String>,

    #[serde(rename = "epochStart", deserialize_with = "deserialize_u64")]
    pub epoch_start: u64,

    #[serde(rename = "epochStartOld", deserialize_with = "deserialize_u64")]
    pub old_epoch_start: u64,

    #[serde(rename = "oldTaps", deserialize_with = "deserialize_u128_vec")]
    pub old_taps: Vec<u128>,

    #[serde(rename = "oldTapsSize", deserialize_with = "deserialize_u128")]
    pub old_taps_size: u128,

    #[serde(rename = "oldMbiCurTaps", deserialize_with = "deserialize_u64_vec")]
    pub old_mbi_cur_taps: Vec<u64>,

    #[serde(rename = "taps", deserialize_with = "deserialize_u128_vec")]
    pub taps: Vec<u128>,

    #[serde(rename = "mbiCurTaps", deserialize_with = "deserialize_u64_vec")]
    pub mbi_cur_taps: Vec<u64>,

    #[serde(rename = "tapsSize", deserialize_with = "deserialize_u128")]
    pub taps_size: u128,

    #[serde(rename = "tapSum", deserialize_with = "deserialize_u128")]
    pub taps_sum: u128,

    #[serde(rename = "modifiedTapSum", deserialize_with = "deserialize_u128")]
    pub modified_taps_sum: u128,

    #[serde(rename = "miningDurSum", deserialize_with = "deserialize_u128")]
    pub mining_duration_sum: u128,

    #[serde(rename = "epochBigStart", deserialize_with = "deserialize_u64")]
    pub big_epoch_start: u64,

    pub seed: String,

    #[serde(rename = "seedNext")]
    pub next_seed: String,

    #[serde(rename = "commitData")]
    pub submit_session_data: Option<String>,

    #[serde(rename = "blockLimitData")]
    pub block_limit_data: Option<String>,

    #[serde(rename = "commitInterval")]
    pub verify_session_interval: Option<(SessionInterval, SessionInterval)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfEncodeSetOwnerPublic {
    #[serde(rename(serialize = "pubkey"))]
    pub public: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSubmitSession {
    #[serde(rename(serialize = "easyNumber"))]
    pub easy_count: u64,

    #[serde(rename(serialize = "tapNumber"))]
    pub hard_count: u64,

    #[serde(rename(serialize = "workerId"))]
    pub worker_id: u64,

    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfVerifySession {
    #[serde(rename(serialize = "verifyData"))]
    pub data: String,

    #[serde(rename(serialize = "eventAddrSuccess"))]
    pub success_event_address: Vec<String>,

    #[serde(rename(serialize = "eventCellSuccess"))]
    pub success_event_data: Vec<String>,

    #[serde(rename(serialize = "eventAddrFailed"))]
    pub error_event_address: Vec<String>,

    #[serde(rename(serialize = "eventCellFailed"))]
    pub error_event_data: Vec<String>,
}

impl Miner {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    /// # Get contract state data
    pub async fn get_details(&self) -> anyhow::Result<ResultOfGetDetails> {
        let call_set =
            CallSet { function_name: "getDetails".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetDetails>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Encode set owner public key message
    /// This key is used to sign messages for miner
    ///
    /// Original contract method: `setOwnerPubkey`
    pub async fn set_owner_public_message(
        &self,
        params: ParamsOfEncodeSetOwnerPublic,
    ) -> anyhow::Result<String> {
        let call_set = CallSet {
            function_name: "setOwnerPubkey".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self
            .encode_message_body(call_set, true, Signer::None)
            .await
            .map_err(|e| anyhow!("Encode message body ({e})"))?;

        Ok(result.body)
    }

    /// # Collect existing rewards
    ///
    /// Original contract method: `getReward`
    pub async fn get_reward(&self, signer: Signer) -> anyhow::Result<ResultOfSendMessage> {
        let call_set =
            CallSet { function_name: "getReward".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Send merkle tree root and total leaves count
    ///
    /// Original contract method: `setCommitData`
    pub async fn submit_session(
        &self,
        params: ParamsOfSubmitSession,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setCommitData".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Send merkle tree proofs
    ///
    /// Original contract method: `acceptTap`
    pub async fn verify_session(
        &self,
        params: ParamsOfVerifySession,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "acceptTap".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
