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
use crate::error::KitModule;
use crate::error::MvSystemModule;
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

const ABI: &str = include_str!("../../abi/mvsystem/Indexer.abi.json");

#[derive(Debug, Clone)]
pub struct Indexer {
    context: Arc<ClientContext>,
    address: String,
    dapp_id: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for Indexer {
    const MODULE: KitModule = KitModule::MvSystem(MvSystemModule::Indexer);
}

impl AccountAccessor for Indexer {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Indexer {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Indexer {
    fn address(&self) -> &str {
        &self.address
    }

    fn dapp_id(&self) -> &str {
        &self.dapp_id
    }
}

impl ContextAccessor for Indexer {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for Indexer {}

impl DecodeMessage for Indexer {}

impl Executor for Indexer {}

impl SendMessage for Indexer {}

impl AsyncGuarded<Account> for Indexer {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Indexer {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetDetails {
    pub name: String,
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetOwner {
    #[serde(rename(serialize = "wallet"))]
    pub multifactor_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamsOfDeployMultifactor {
    #[serde(rename(serialize = "wallet"))]
    pub multifactor_address: String,
    pub zkid: String,
    pub proof: String,
    pub epk: String,
    pub epk_sig: String,
    pub epk_expire_at: u64,
    pub jwk_modulus: String,
    pub kid: String,
    pub jwk_modulus_expire_at: u64,
    pub index_mod_4: u8,
    pub iss_base_64: String,
    pub provider: String,
    pub header_base_64: String,
    pub pub_recovery_key: String,
    pub pub_recovery_key_sig: String,
    pub jwk_update_key: String,
    pub jwk_update_key_sig: String,
    pub root_provider_certificates: HashMap<String, String>,
    pub owner_pubkey: String,
    pub mirror: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfEncodeSetOwner {
    #[serde(rename(serialize = "wallet"))]
    pub multifactor_address: String,
}

impl Indexer {
    pub fn new(
        context: Arc<ClientContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        let params = params.into();
        Self {
            context: context.clone(),
            address: params.address.clone(),
            dapp_id: params.dapp_id.clone(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, &params.address, params.dapp_id))),
        }
    }

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// # Encode set indexer owner message
    ///
    /// Original contract method: `setNewWallet`
    ///
    /// Should be sent from current owner multifactor
    pub async fn set_owner_message(&self, params: ParamsOfEncodeSetOwner) -> KitResult<String> {
        let call_set = CallSet {
            function_name: "setNewWallet".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self.encode_message_body(call_set, true, Signer::None).await?;

        Ok(result.body)
    }

    /// # Deploy multifactor
    ///
    /// Used to create multifactor and claim current indexer
    ///
    /// Original contract method: `isOwnerRoot`
    ///
    /// Should be signed with root keys
    pub async fn deploy_multifactor(
        &self,
        params: ParamsOfDeployMultifactor,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "isOwnerRoot".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
