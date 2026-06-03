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
use crate::deserialize::deserialize_u64;
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
use crate::traits::VersionAccessor;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/mvsystem/Boost.abi.json");
const ABI_1_0_1: &str = include_str!("../../abi/mvsystem/Boost_1.0.1.abi.json");

#[derive(Debug, Clone)]
pub struct Boost {
    context: Arc<ClientContext>,
    address: String,
    dapp_id: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for Boost {
    const MODULE: KitModule = KitModule::MvSystem(MvSystemModule::Boost);
}

impl AccountAccessor for Boost {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Boost {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Boost {
    fn address(&self) -> &str {
        &self.address
    }

    fn dapp_id(&self) -> &str {
        &self.dapp_id
    }
}

impl ContextAccessor for Boost {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl VersionAccessor for Boost {}

impl EncodeMessage for Boost {}

impl DecodeMessage for Boost {}

impl Executor for Boost {}

impl SendMessage for Boost {}

impl AsyncGuarded<Account> for Boost {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Boost {
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
    #[serde(rename = "mbiCur", deserialize_with = "deserialize_u64")]
    pub mamaboard_max_seq_no: u64,
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetMbiCur {
    #[serde(rename(serialize = "mbiCur"))]
    pub mamaboard_max_seq_no: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfUpdateCode {
    #[serde(rename(serialize = "newcode"))]
    pub code: String,
    pub cell: String,
}

impl Boost {
    pub async fn new(
        context: Arc<ClientContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> KitResult<Self> {
        let params = params.into();
        let version = {
            let instance = Self {
                context: context.clone(),
                address: params.address.clone(),
                dapp_id: params.dapp_id.clone(),
                abi: Abi::Json(ABI.to_string()),
                account: Arc::new(Mutex::new(Account::new(
                    context.clone(),
                    &params.address,
                    params.dapp_id.clone(),
                ))),
            };
            instance.get_version().await?
        };

        let abi = match version.version.as_str() {
            "1.0.1" => ABI_1_0_1,
            _ => ABI,
        };

        Ok(Self {
            context: context.clone(),
            address: params.address.clone(),
            dapp_id: params.dapp_id.clone(),
            abi: Abi::Json(abi.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, &params.address, params.dapp_id))),
        })
    }

    /// Wrapper bound to `address`, under the Mobile Verifiers dApp.
    pub async fn new_default(
        context: Arc<ClientContext>,
        address: impl AsRef<str>,
    ) -> KitResult<Self> {
        Self::new(
            context,
            crate::account::ParamsOfNewContract::new(
                address.as_ref(),
                crate::dapp::SystemDapp::MobileVerifiers,
            ),
        )
        .await
    }

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// # Set mamaboard max detail index
    ///
    /// Original contract method: `setMbiCur`
    ///
    /// Should be signed with root keys
    pub async fn set_mbi_cur(
        &self,
        params: ParamsOfSetMbiCur,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setMbiCur".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Update code
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
