use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
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
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u32;
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

const ABI: &str = include_str!("../../abi/mvsystem/MobileVerifiersContractRoot.abi.json");

#[derive(Debug, Clone)]
pub struct MobileVerifiersRoot {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
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
}

impl ContextAccessor for MobileVerifiersRoot {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for MobileVerifiersRoot {}

impl DecodeMessage for MobileVerifiersRoot {}

impl Executor for MobileVerifiersRoot {}

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
impl AsyncGuarded<Account> for MobileVerifiersRoot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T + 'async_trait,
        T: 'async_trait,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
impl AsyncGuardedMut<Account> for MobileVerifiersRoot {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut + 'async_trait,
        Fut: Future<Output = anyhow::Result<T>> + 'async_trait,
        T: 'async_trait,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone)]
pub struct ParamsOfGetIndexer {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetIndexerAddress {
    #[serde(rename = "indexerAddress")]
    pub address: String,
}

#[derive(Debug, Clone)]
pub struct ParamsOfGetMvMultifactor {
    pub public: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetMvMultifactorAddress {
    #[serde(rename = "mvMultifactorAddress")]
    pub address: String,
}

#[derive(Debug, Clone)]
pub struct ParamsOfGetPopitgame {
    pub multifactor_address: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ResultOfGetPopitgameAddress {
    #[serde(rename = "popitGameAddress")]
    pub address: String,
}

#[derive(Debug, Clone)]
pub struct ParamsOfGetPopcoinRoot {
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
        let address = "0:2222222222222222222222222222222222222222222222222222222222222222";
        Self {
            context: context.clone(),
            address: address.to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    /// # Get multifactor wallet instance
    ///
    /// Original contract method: `getMvMultifactorAddress`
    pub async fn get_mv_multifactor(
        &self,
        params: ParamsOfGetMvMultifactor,
    ) -> anyhow::Result<Multifactor> {
        let call_set = CallSet {
            function_name: "getMvMultifactorAddress".to_string(),
            header: None,
            input: Some(json!({"pubkey": params.public})),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => {
                    let decoded = serde_json::from_value::<ResultOfGetMvMultifactorAddress>(value)
                        .map_err(|e| anyhow!("Deserialize output ({e})"))?;
                    Ok(Multifactor::new(self.context().clone(), decoded.address))
                }
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Get popitgame instance
    ///
    /// Original contract method: `getPopitGameAddress`
    pub async fn get_popitgame(&self, params: ParamsOfGetPopitgame) -> anyhow::Result<Popitgame> {
        let call_set = CallSet {
            function_name: "getPopitGameAddress".to_string(),
            header: None,
            input: Some(json!({"owner": params.multifactor_address})),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => {
                    let decoded = serde_json::from_value::<ResultOfGetPopitgameAddress>(value)
                        .map_err(|e| anyhow!("Deserialize output ({e})"))?;
                    Ok(Popitgame::new(self.context().clone(), decoded.address))
                }
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Get popcoin root instance
    ///
    /// Original contract method: `getPopCoinRootAddress`
    pub async fn get_popcoin_root(
        &self,
        params: ParamsOfGetPopcoinRoot,
    ) -> anyhow::Result<PopcoinRoot> {
        let call_set = CallSet {
            function_name: "getPopCoinRootAddress".to_string(),
            header: None,
            input: Some(json!({"name": params.name})),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => {
                    let decoded = serde_json::from_value::<ResultOfGetPopcoinRootAddress>(value)
                        .map_err(|e| anyhow!("Deserialize output ({})", e))?;
                    Ok(PopcoinRoot::new(self.context().clone(), decoded.address))
                }
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Get indexer instance
    ///
    /// Original contract method: `getIndexerAddress`
    pub async fn get_indexer(&self, params: ParamsOfGetIndexer) -> anyhow::Result<Indexer> {
        let call_set = CallSet {
            function_name: "getIndexerAddress".to_string(),
            header: None,
            input: Some(json!({"name": params.name})),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => {
                    let decoded = serde_json::from_value::<ResultOfGetIndexerAddress>(value)
                        .map_err(|e| anyhow!("Deserialize output ({e})"))?;
                    Ok(Indexer::new(self.context().clone(), decoded.address))
                }
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Get indexer code
    ///
    /// Original contract method: `getIndexerCode`
    pub async fn get_indexer_code(&self) -> anyhow::Result<ResultOfGetIndexerCode> {
        let call_set =
            CallSet { function_name: "getIndexerCode".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetIndexerCode>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Get all codes
    ///
    /// Original contract method: `getCodes`
    pub async fn get_codes(&self) -> anyhow::Result<ResultOfGetCodes> {
        let call_set = CallSet { function_name: "getCodes".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetCodes>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Get epoch start/end unixtime
    ///
    /// Original contract method: `getEpoch`
    pub async fn get_epoch(&self) -> anyhow::Result<ResultOfGetEpoch> {
        let call_set = CallSet { function_name: "getEpoch".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetEpoch>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }
}
