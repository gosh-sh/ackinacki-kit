use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u128_map;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/PopCoinWallet.abi.json");

#[derive(Debug, Clone)]
pub struct PopcoinWallet {
    abi: Abi,
    account: Account,
}

impl AccountAccessor for PopcoinWallet {
    fn account(&self) -> &Account {
        &self.account
    }
}

impl AbiAccessor for PopcoinWallet {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for PopcoinWallet {
    fn address(&self) -> &str {
        &self.account.address
    }
}

impl ContextAccessor for PopcoinWallet {
    fn context(&self) -> Arc<ClientContext> {
        self.account.context.clone()
    }
}

impl EncodeMessage for PopcoinWallet {}

impl Executor for PopcoinWallet {}

impl SendMessage for PopcoinWallet {}

#[derive(Debug, Deserialize)]
pub struct ResultOfGetDetails {
    pub name: String,
    #[serde(rename = "popcoinroot")]
    pub root_address: String,
    #[serde(rename = "owner")]
    pub multifactor_address: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub value: u128,
    #[serde(rename = "isReady")]
    pub is_ready: bool,
    #[serde(rename = "isPlay")]
    pub is_play: bool,
    #[serde(rename = "gameAddress")]
    pub game_address: String,
    #[serde(rename = "popits_candidate", deserialize_with = "deserialize_u128_map")]
    pub popits_candidates: HashMap<String, u128>,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfAddPopitCandidate {
    /// Key from popcoin candidates map
    #[serde(rename = "id")]
    pub candidate_key: String,

    /// Amount of popcoin that should be added
    #[serde(rename = "value")]
    pub amount: u128,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfAddValue {
    #[serde(rename = "value")]
    pub amount: u128,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfEncodeActivatePopitCandidate {
    /// Key from wallet `popits_candidate` map
    #[serde(rename = "id")]
    pub candidate_key: String,

    /// Key from popcoin `popits_media` map
    #[serde(rename = "indexRoot")]
    pub issued_key: u16,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfEncodeDeletePopitCandidate {
    /// Key from wallet `popits_candidate` map
    #[serde(rename = "index")]
    pub candidate_key: String,
}

impl PopcoinWallet {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { abi: Abi::Json(ABI.to_string()), account: Account::new(context, address) }
    }

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

    /// # Encode activate wallet message body
    ///
    /// Popcoin should be already been activated
    ///
    /// Original contract method: `activate`
    ///
    /// Should be sent from owner multifactor contract
    pub async fn activate_message(&self) -> anyhow::Result<String> {
        let call_set = CallSet { function_name: "activate".to_string(), header: None, input: None };

        let result = self
            .encode_message_body(call_set, true, Signer::None)
            .await
            .map_err(|e| anyhow!("Encode message body ({e})"))?;

        Ok(result.body)
    }

    /// # Encode activate popit candidate message body
    ///
    /// Candidate should be already been activated in popcoin
    ///
    /// Original contract method: `activatePopit`
    ///
    /// Should be sent from owner multifactor contract
    pub async fn activate_popit_candidate_message(
        &self,
        params: ParamsOfEncodeActivatePopitCandidate,
    ) -> anyhow::Result<String> {
        let call_set = CallSet {
            function_name: "activatePopit".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self
            .encode_message_body(call_set, true, Signer::None)
            .await
            .map_err(|e| anyhow!("Encode message body ({e})"))?;

        Ok(result.body)
    }

    /// # Encode message body to delete popit candidate
    ///
    /// Original contract method: `deleteCandidate`
    ///
    /// Should be sent from owner multifactor contract
    pub async fn delete_popit_candidate_message(
        &self,
        params: ParamsOfEncodeDeletePopitCandidate,
    ) -> anyhow::Result<String> {
        let call_set = CallSet {
            function_name: "deleteCandidate".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self
            .encode_message_body(call_set, true, Signer::None)
            .await
            .map_err(|e| anyhow!("Encode message body ({e})"))?;

        Ok(result.body)
    }

    /// # Encode message body to destroy wallet account
    ///
    /// Original contract method: `destroy`
    ///
    /// Should be sent from owner multifactor contract
    pub async fn destroy_message(&self) -> anyhow::Result<String> {
        let call_set = CallSet { function_name: "destroy".to_string(), header: None, input: None };

        let result = self
            .encode_message_body(call_set, true, Signer::None)
            .await
            .map_err(|e| anyhow!("Encode message body ({e})"))?;

        Ok(result.body)
    }

    /// # Add amount of popcoin to this wallet (no rewards will be given)
    ///
    /// Original contract method: `addValueOld`
    ///
    /// Should be signed with server keys
    pub async fn add_value(
        &self,
        params: ParamsOfAddValue,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "addValueOld".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Add popit candidate amount to this wallet
    ///
    /// Original contract method: `addValue`
    ///
    /// Should be signed with server keys
    pub async fn add_popit_candidate(
        &self,
        params: ParamsOfAddPopitCandidate,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "addValue".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
