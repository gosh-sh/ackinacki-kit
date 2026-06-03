use std::collections::HashMap;
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
use crate::deserialize::deserialize_u32;
use crate::error::KitModule;
use crate::error::MvSystemModule;
use crate::mvsystem::indexer::Indexer;
use crate::mvsystem::multifactor::Multifactor;
use crate::mvsystem::popcoin_root::PopcoinRoot;
use crate::mvsystem::popitgame::Popitgame;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::GetMethodAccessor;
use crate::traits::ModuleAccessor;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/mvsystem/MobileVerifiersContractRoot.abi.json");

#[derive(Debug, Clone)]
pub struct MobileVerifiersRoot {
    context: Arc<ClientContext>,
    address: String,
    dapp_id: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for MobileVerifiersRoot {
    const MODULE: KitModule = KitModule::MvSystem(MvSystemModule::Root);
}

impl AccountAccessor for MobileVerifiersRoot {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for MobileVerifiersRoot {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for MobileVerifiersRoot {
    fn address(&self) -> &str {
        &self.address
    }

    fn dapp_id(&self) -> &str {
        &self.dapp_id
    }
}

impl ContextAccessor for MobileVerifiersRoot {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for MobileVerifiersRoot {}

impl DecodeMessage for MobileVerifiersRoot {}

impl Executor for MobileVerifiersRoot {}

impl AsyncGuarded<Account> for MobileVerifiersRoot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for MobileVerifiersRoot {
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
pub struct ParamsOfGetIndexer {
    #[serde(rename = "name")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetIndexerAddress {
    #[serde(rename = "indexerAddress")]
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetMvMultifactor {
    #[serde(rename = "pubkey")]
    pub public: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetMvMultifactorAddress {
    #[serde(rename = "mvMultifactorAddress")]
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetPopitgame {
    #[serde(rename = "owner")]
    pub multifactor_address: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ResultOfGetPopitgameAddress {
    #[serde(rename = "popitGameAddress")]
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetPopcoinRoot {
    #[serde(rename = "name")]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetPopcoinRootAddress {
    #[serde(rename = "popCoinRootAddress")]
    address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetIndexerCode {
    #[serde(rename = "data")]
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetCodes {
    #[serde(rename = "code")]
    pub codes: HashMap<u8, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetEpoch {
    #[serde(rename = "epochStart", deserialize_with = "deserialize_u32")]
    pub start: u32,
    #[serde(rename = "epochEnd", deserialize_with = "deserialize_u32")]
    pub end: u32,
}

impl MobileVerifiersRoot {
    pub fn new(context: Arc<ClientContext>) -> Self {
        Self::with_dapp_id(context, crate::dapp::SystemDapp::MobileVerifiers)
    }

    /// Like [`Self::new`] but with a caller-supplied dApp ID.
    pub fn with_dapp_id(context: Arc<ClientContext>, dapp_id: impl Into<String>) -> Self {
        let address = "0:2222222222222222222222222222222222222222222222222222222222222222";
        let dapp_id = dapp_id.into();
        Self {
            context: context.clone(),
            address: address.to_string(),
            dapp_id: dapp_id.clone(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address, dapp_id))),
        }
    }

    /// # Get multifactor wallet instance
    ///
    /// Original contract method: `getMvMultifactorAddress`
    pub async fn get_mv_multifactor(
        &self,
        params: ParamsOfGetMvMultifactor,
    ) -> KitResult<Multifactor> {
        let res_of_get_addr = self
            .call_get_method_with::<ResultOfGetMvMultifactorAddress, ParamsOfGetMvMultifactor>(
                "getMvMultifactorAddress",
                params,
            )
            .await?;

        Ok(Multifactor::new(
            self.context().clone(),
            crate::account::ParamsOfNewContract::new(res_of_get_addr.address, self.dapp_id()),
        ))
    }

    /// # Get popitgame instance
    ///
    /// Original contract method: `getPopitGameAddress`
    pub async fn get_popitgame(&self, params: ParamsOfGetPopitgame) -> KitResult<Popitgame> {
        let res_of_get_addr = self
            .call_get_method_with::<ResultOfGetPopitgameAddress, ParamsOfGetPopitgame>(
                "getPopitGameAddress",
                params,
            )
            .await?;

        Ok(Popitgame::new(
            self.context().clone(),
            crate::account::ParamsOfNewContract::new(res_of_get_addr.address, self.dapp_id()),
        ))
    }

    /// # Get popcoin root instance
    ///
    /// Original contract method: `getPopCoinRootAddress`
    pub async fn get_popcoin_root(&self, params: ParamsOfGetPopcoinRoot) -> KitResult<PopcoinRoot> {
        let res_of_get_addr = self
            .call_get_method_with::<ResultOfGetPopcoinRootAddress, ParamsOfGetPopcoinRoot>(
                "getPopCoinRootAddress",
                params,
            )
            .await?;

        Ok(PopcoinRoot::new(
            self.context().clone(),
            crate::account::ParamsOfNewContract::new(res_of_get_addr.address, self.dapp_id()),
        ))
    }

    /// # Get indexer instance
    ///
    /// Original contract method: `getIndexerAddress`
    pub async fn get_indexer(&self, params: ParamsOfGetIndexer) -> KitResult<Indexer> {
        let res_of_get_addr = self
            .call_get_method_with::<ResultOfGetIndexerAddress, ParamsOfGetIndexer>(
                "getIndexerAddress",
                params,
            )
            .await?;

        Ok(Indexer::new(
            self.context().clone(),
            crate::account::ParamsOfNewContract::new(res_of_get_addr.address, self.dapp_id()),
        ))
    }

    /// # Get indexer code
    ///
    /// Original contract method: `getIndexerCode`
    pub async fn get_indexer_code(&self) -> KitResult<ResultOfGetIndexerCode> {
        self.call_get_method::<ResultOfGetIndexerCode>("getIndexerCode").await
    }

    /// # Get all codes
    ///
    /// Original contract method: `getCodes`
    pub async fn get_codes(&self) -> KitResult<ResultOfGetCodes> {
        self.call_get_method::<ResultOfGetCodes>("getCodes").await
    }

    /// # Get epoch start/end unixtime
    ///
    /// Original contract method: `getEpoch`
    pub async fn get_epoch(&self) -> KitResult<ResultOfGetEpoch> {
        self.call_get_method::<ResultOfGetEpoch>("getEpoch").await
    }
}
