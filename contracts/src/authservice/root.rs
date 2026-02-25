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
use crate::authservice::profile::AuthProfile;
use crate::error::AuthServiceModule;
use crate::error::KitModule;
use crate::traits::AutoContract;
use crate::traits::AccountAccessor;
use crate::traits::ContractBase;
use crate::traits::ContextAccessor;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/authservice/AuthServiceRoot.abi.json");

/// Reference wrapper migrated to the reduced-boilerplate style:
/// - stores shared runtime state in `ContractBase`
/// - exposes it via `HasContractBase`
/// - keeps contract identity in `ModuleAccessor`
/// - opts into blanket message/executor impls via `AutoContract`
#[derive(Debug, Clone)]
pub struct AuthServiceRoot {
    base: ContractBase,
}

impl ModuleAccessor for AuthServiceRoot {
    const MODULE: KitModule = KitModule::AuthService(AuthServiceModule::Root);
}

impl HasContractBase for AuthServiceRoot {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for AuthServiceRoot {}

impl AsyncGuarded<Account> for AuthServiceRoot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for AuthServiceRoot {
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
pub struct ParamsOfSetProfileCode {
    pub code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeployProfile {
    pub pubkey: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetProfileAddress {
    pub pubkey: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetProfileAddress {
    pub profile: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfUpdateCode {
    #[serde(rename(serialize = "newcode"))]
    pub new_code: String,
    pub cell: String,
}

impl AuthServiceRoot {
    pub const DEFAULT_ADDRESS: &'static str =
        "0:0404040404040404040404040404040404040404040404040404040404040404";

    /// Allows passing the root address explicitly (useful for shellnet/testnet
    /// or local networks where AuthServiceRoot may live at a non-premine address).
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            base: ContractBase::new(context, address, Abi::Json(ABI.to_string())),
        }
    }

    pub fn new_default(context: Arc<ClientContext>) -> Self {
        Self::new(context, Self::DEFAULT_ADDRESS)
    }

    /// # Set auth profile code
    ///
    /// Original contract method: `setProfileCode`
    ///
    /// Should be signed with root keys
    pub async fn set_profile_code(
        &self,
        params: ParamsOfSetProfileCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setProfileCode".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy auth profile
    ///
    /// Original contract method: `deployProfile`
    ///
    /// Open method, can be called by any external sender
    pub async fn deploy_profile(
        &self,
        params: ParamsOfDeployProfile,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployProfile".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Get auth profile address
    ///
    /// Original contract method: `getProfileAddress`
    pub async fn get_profile_address(
        &self,
        params: ParamsOfGetProfileAddress,
    ) -> KitResult<ResultOfGetProfileAddress> {
        self.call_get_method_with::<ResultOfGetProfileAddress, ParamsOfGetProfileAddress>(
            "getProfileAddress",
            params,
        )
        .await
    }

    /// # Get auth profile instance
    ///
    /// Original contract method: `getProfileAddress`
    pub async fn get_profile(&self, params: ParamsOfGetProfileAddress) -> KitResult<AuthProfile> {
        let profile = self.get_profile_address(params).await?;
        Ok(AuthProfile::new(self.context().clone(), profile.profile))
    }

    /// # Update root code
    ///
    /// Original contract method: `updateCode`
    ///
    /// Should be signed with root keys
    pub async fn update_code(
        &self,
        params: ParamsOfUpdateCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "updateCode".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
