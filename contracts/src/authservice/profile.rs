use std::sync::Arc;

use serde::Deserialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::error::AuthServiceModule;
use crate::error::KitModule;
use crate::traits::AutoContract;
use crate::traits::AccountAccessor;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/authservice/AuthProfile.abi.json");

#[derive(Debug, Clone)]
pub struct AuthProfile {
    base: ContractBase,
}

impl ModuleAccessor for AuthProfile {
    const MODULE: KitModule = KitModule::AuthService(AuthServiceModule::Profile);
}

impl HasContractBase for AuthProfile {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for AuthProfile {}

impl AsyncGuarded<Account> for AuthProfile {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for AuthProfile {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
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
            base: ContractBase::new(context, address, Abi::Json(ABI.to_string())),
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
