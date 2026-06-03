use std::sync::Arc;

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
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/mvconfig/MVConfig.abi.json");

#[derive(Debug, Clone)]
pub struct MobileVerifiersConfig {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for MobileVerifiersConfig {
    const MODULE: KitModule = KitModule::MvConfig;
}

impl AccountAccessor for MobileVerifiersConfig {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for MobileVerifiersConfig {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for MobileVerifiersConfig {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for MobileVerifiersConfig {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for MobileVerifiersConfig {}

impl DecodeMessage for MobileVerifiersConfig {}

impl Executor for MobileVerifiersConfig {}

impl SendMessage for MobileVerifiersConfig {}

impl AsyncGuarded<Account> for MobileVerifiersConfig {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for MobileVerifiersConfig {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetConfig {
    #[serde(rename(serialize = "MBNLst"))]
    pub mbn_list: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetRootPublic {
    #[serde(rename(serialize = "pubkey"))]
    pub public: String,
}

impl MobileVerifiersConfig {
    pub fn new(context: Arc<ClientContext>) -> Self {
        Self::with_dapp_id(context, crate::dapp::SystemDapp::MobileVerifiers)
    }

    /// Like [`Self::new`] but with a caller-supplied dApp ID.
    pub fn with_dapp_id(context: Arc<ClientContext>, dapp_id: impl Into<String>) -> Self {
        let address = "0:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        Self {
            context: context.clone(),
            address: address.to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address, Some(dapp_id.into())))),
        }
    }

    /// # Set root public key
    ///
    /// Original contract method: `setPubkeyRoot`
    ///
    /// Should be signed with root keys
    pub async fn set_root_public(
        &self,
        params: ParamsOfSetRootPublic,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setPubkeyRoot".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Set config
    ///
    /// Original contract method: `setConfig`
    ///
    /// Should be signed with root keys
    pub async fn set_config(
        &self,
        params: ParamsOfSetConfig,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setConfig".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
