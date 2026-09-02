use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;

use crate::account::Account;
use crate::delivery::ContractContext;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/mvconfig/MVConfig.abi.json");

#[derive(Debug, Clone)]
pub struct MobileVerifiersConfig {
    base: ContractBase,
}

impl ModuleAccessor for MobileVerifiersConfig {
    const MODULE: KitModule = KitModule::MvConfig;
}

impl HasContractBase for MobileVerifiersConfig {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for MobileVerifiersConfig {}

impl AsyncGuarded<Account> for MobileVerifiersConfig {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for MobileVerifiersConfig {
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
    /// General constructor — caller supplies address + dApp ID.
    pub fn new(
        context: impl Into<ContractContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())) }
    }

    /// Wrapper bound to the default address, under the Mobile Verifiers dApp.
    pub fn new_default(context: impl Into<ContractContext>) -> Self {
        Self::new(
            context,
            crate::account::ParamsOfNewContract::new(
                "0:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                crate::dapp::SystemDapp::MobileVerifiers,
            ),
        )
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
