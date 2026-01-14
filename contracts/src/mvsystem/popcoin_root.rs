use std::collections::HashMap;
use std::sync::Arc;

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
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u16;
use crate::deserialize::deserialize_u64;
use crate::error::KitModule;
use crate::error::MvSystemModule;
use crate::mvsystem::Popit;
use crate::mvsystem::PopitCandidateWithMedia;
use crate::mvsystem::PopitMedia;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::GetMethodAccessor;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/mvsystem/PopCoinRoot.abi.json");

#[derive(Debug, Clone)]
pub struct PopcoinRoot {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for PopcoinRoot {
    const MODULE: KitModule = KitModule::MvSystem(MvSystemModule::PopcoinRoot);
}

impl AccountAccessor for PopcoinRoot {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for PopcoinRoot {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for PopcoinRoot {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for PopcoinRoot {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for PopcoinRoot {}

impl DecodeMessage for PopcoinRoot {}

impl Executor for PopcoinRoot {}

impl SendMessage for PopcoinRoot {}

impl AsyncGuarded<Account> for PopcoinRoot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for PopcoinRoot {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetDetails {
    pub name: String,
    pub root: String,
    #[serde(rename = "totalSupply", deserialize_with = "deserialize_u128")]
    pub total_supply: u128,
    #[serde(rename = "maxPopitIndex", deserialize_with = "deserialize_u16")]
    pub max_popit_index: u16,
    pub popits_value: HashMap<u16, Popit>,
    pub popits_media: HashMap<u16, PopitMedia>,
    #[serde(rename = "popits_candidate")]
    pub popits_candidates: HashMap<String, PopitCandidateWithMedia>,
    #[serde(deserialize_with = "deserialize_u128")]
    pub rewards: u128,
    #[serde(rename = "isReadyStatus")]
    pub is_ready: bool,
    #[serde(rename = "popitGameOwner")]
    pub owner_popitgame_address: String,
    pub description: String,
    #[serde(rename = "deployed", deserialize_with = "deserialize_u64")]
    pub deployed_seq_no: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfActivate {
    #[serde(rename(serialize = "isOld"))]
    pub is_old: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfAddPopitCandidate {
    #[serde(rename(serialize = "media"))]
    pub file_id: String,
    #[serde(rename = "protopopit")]
    pub proto_id: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfActivatePopitCandidate {
    #[serde(rename(serialize = "id"))]
    pub candidate_key: String,
    #[serde(rename(serialize = "media"))]
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeletePopitCandidate {
    #[serde(rename(serialize = "id"))]
    pub candidate_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetIsPublic {
    #[serde(rename(serialize = "isPublic"))]
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetPopitMedia {
    #[serde(rename(serialize = "index"))]
    pub key: u16,
    pub data: PopitMedia,
}

impl PopcoinRoot {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// # Set `public` flag for popcoin
    ///
    /// This flag controls who can add popits to popcoin (only owner or anyone)
    ///
    /// Original contract method: `setIsPublic`
    ///
    /// Should be signed with server keys
    pub async fn set_is_public(
        &self,
        params: ParamsOfSetIsPublic,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setIsPublic".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Set media for issued popit in popcoin
    ///
    /// Original contract method: `setPopitMedia`
    ///
    /// Should be signed with server keys
    pub async fn set_popit_media(
        &self,
        params: ParamsOfSetPopitMedia,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setPopitMedia".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Activate popcoin
    ///
    /// Original contract method: `activate`
    ///
    /// Should be signed with server keys
    pub async fn activate(
        &self,
        params: ParamsOfActivate,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "activate".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Add popit candidate to popcoin
    ///
    /// Original contract method: `addNewPopit`
    ///
    /// Should be signed with server keys
    pub async fn add_popit_candidate(
        &self,
        params: ParamsOfAddPopitCandidate,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "addNewPopit".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Activate popit candidate
    ///
    /// Original contract method: `activatePopit`
    ///
    /// Should be signed with server keys
    pub async fn activate_popit_candidate(
        &self,
        params: ParamsOfActivatePopitCandidate,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "activatePopit".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Delete popit candidate
    ///
    /// Original contract method: `deleteCandidate`
    ///
    /// Should be signed with server keys
    pub async fn delete_popit_candidate(
        &self,
        params: ParamsOfDeletePopitCandidate,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deleteCandidate".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Delete popcoin
    ///
    /// Original contract method: `destroy`
    ///
    /// Should be signed with server keys
    pub async fn destroy(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet { function_name: "destroy".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }
}
