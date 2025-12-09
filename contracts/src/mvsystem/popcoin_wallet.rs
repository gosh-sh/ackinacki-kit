use std::collections::HashMap;
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
use crate::deserialize::deserialize_u64;
use crate::deserialize::deserialize_u64_map;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/PopCoinWallet.abi.json");

#[derive(Debug, Clone)]
pub struct PopcoinWallet {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for PopcoinWallet {
    fn account(&self) -> &Arc<Mutex<Account>> {
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
        &self.address
    }
}

impl ContextAccessor for PopcoinWallet {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for PopcoinWallet {}

impl DecodeMessage for PopcoinWallet {}

impl Executor for PopcoinWallet {}

impl SendMessage for PopcoinWallet {}

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
impl AsyncGuarded<Account> for PopcoinWallet {
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
impl AsyncGuardedMut<Account> for PopcoinWallet {
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
    pub name: String,
    #[serde(rename = "popcoinroot")]
    pub root_address: String,
    #[serde(rename = "owner")]
    pub multifactor_address: String,
    #[serde(deserialize_with = "deserialize_u64")]
    pub value: u64,
    #[serde(rename = "isReady")]
    pub is_ready: bool,
    #[serde(rename = "popits_candidate", deserialize_with = "deserialize_u64_map")]
    pub popits_candidates: HashMap<String, u64>,
    #[serde(rename = "popits_mbi", deserialize_with = "deserialize_u64_map")]
    pub popits_mbi: HashMap<String, u64>,
    #[serde(rename = "deployed", deserialize_with = "deserialize_u64")]
    pub deployed_seq_no: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfAddPopitCandidate {
    /// Key from popcoin candidates map
    #[serde(rename(serialize = "id"))]
    pub candidate_key: String,

    /// Amount of popcoin that should be added
    #[serde(rename(serialize = "value"))]
    pub amount: u64,

    /// Mamaboard current level
    #[serde(rename(serialize = "mbiCur"))]
    pub mbi_cur: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfAddValue {
    #[serde(rename(serialize = "value"))]
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfEncodeActivatePopitCandidate {
    /// Key from wallet `popits_candidate` map
    #[serde(rename(serialize = "id"))]
    pub candidate_key: String,

    /// Key from popcoin `popits_media` map
    #[serde(rename(serialize = "indexRoot"))]
    pub issued_key: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfEncodeDeletePopitCandidate {
    /// Key from wallet `popits_candidate` map
    #[serde(rename(serialize = "index"))]
    pub candidate_key: String,
}

impl PopcoinWallet {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
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

    /// # Destroy account
    ///
    /// Original contract method: `destroyRoot`
    ///
    /// Should be signed with server keys
    pub async fn destroy(&self, signer: Signer) -> anyhow::Result<ResultOfSendMessage> {
        let call_set =
            CallSet { function_name: "destroyRoot".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
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
