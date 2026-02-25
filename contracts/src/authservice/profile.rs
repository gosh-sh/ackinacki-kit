use std::sync::Arc;

use serde::Deserialize;
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
use crate::error::AuthServiceModule;
use crate::error::KitModule;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeAccountData;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::GetMethodAccessor;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::traits::VersionAccessor;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/authservice/AuthProfile.abi.json");

#[derive(Debug, Clone)]
pub struct AuthProfile {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for AuthProfile {
    const MODULE: KitModule = KitModule::AuthService(AuthServiceModule::Profile);
}

impl AccountAccessor for AuthProfile {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for AuthProfile {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for AuthProfile {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for AuthProfile {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl VersionAccessor for AuthProfile {}

impl EncodeMessage for AuthProfile {}

impl DecodeMessage for AuthProfile {}

impl DecodeAccountData<serde_json::Value> for AuthProfile {}

impl Executor for AuthProfile {}

impl SendMessage for AuthProfile {}

impl AsyncGuarded<Account> for AuthProfile {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for AuthProfile {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetDetails {
    pub description: String,
    #[serde(rename = "pubkeyHash")]
    pub pubkey_hash: String,
    pub pubkey: String,
    pub root: String,
}

impl AuthProfile {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    /// # Get profile details
    ///
    /// Original contract method: `getDetails`
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// # Destroy profile
    ///
    /// Original contract method: `destroy`
    ///
    /// Should be signed with profile owner keys
    pub async fn destroy(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet { function_name: "destroy".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }
}
