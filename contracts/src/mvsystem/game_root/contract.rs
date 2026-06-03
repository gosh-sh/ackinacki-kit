use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::error::KitModule;
use crate::error::MvSystemModule;
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
use crate::KitResult;

const ABI: &str = include_str!("../../../abi/mvsystem/MobileVerifiersContractGameRoot.abi.json");

#[derive(Debug, Clone)]
pub struct MobileVerifiersGameRoot {
    context: Arc<ClientContext>,
    address: String,
    dapp_id: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for MobileVerifiersGameRoot {
    const MODULE: KitModule = KitModule::MvSystem(MvSystemModule::GameRoot);
}

impl AccountAccessor for MobileVerifiersGameRoot {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for MobileVerifiersGameRoot {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for MobileVerifiersGameRoot {
    fn address(&self) -> &str {
        &self.address
    }

    fn dapp_id(&self) -> &str {
        &self.dapp_id
    }
}

impl ContextAccessor for MobileVerifiersGameRoot {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for MobileVerifiersGameRoot {}

impl DecodeMessage for MobileVerifiersGameRoot {}

impl DecodeAccountData<serde_json::Value> for MobileVerifiersGameRoot {}

impl Executor for MobileVerifiersGameRoot {}

impl AsyncGuarded<Account> for MobileVerifiersGameRoot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for MobileVerifiersGameRoot {
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
pub struct ParamsOfGetCellForBoost {
    #[serde(rename(serialize = "wallet"))]
    pub wallet_address: String,

    #[serde(rename(serialize = "popitGame"))]
    pub popitgame_address: String,

    #[serde(rename(serialize = "root"))]
    pub root_address: String,

    #[serde(rename(serialize = "mbiCur"))]
    pub mbi_cur: u64,

    #[serde(rename(serialize = "rootPubkey"))]
    pub root_public: String,

    #[serde(rename(serialize = "miner"))]
    pub miner_address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetCellForBoost {
    #[serde(rename = "value0")]
    pub cell: String,
}

impl MobileVerifiersGameRoot {
    /// General constructor — caller supplies address + dApp ID.
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

    /// Wrapper bound to the default address, under the Mobile Verifiers dApp.
    pub fn new_default(context: Arc<ClientContext>) -> Self {
        Self::new(
            context,
            crate::account::ParamsOfNewContract::new(
                "0:0505050505050505050505050505050505050505050505050505050505050505",
                crate::dapp::SystemDapp::MobileVerifiers,
            ),
        )
    }

    /// # Get cell for boost contract to upgrade code
    ///
    /// Original contract method: `getCellForBoost`
    pub async fn get_cell_for_boost(
        &self,
        params: ParamsOfGetCellForBoost,
    ) -> KitResult<ResultOfGetCellForBoost> {
        self.call_get_method_with::<ResultOfGetCellForBoost, ParamsOfGetCellForBoost>(
            "getCellForBoost",
            params,
        )
        .await
    }
}
