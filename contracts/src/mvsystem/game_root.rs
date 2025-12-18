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
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeAccountData;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;

const ABI: &str = include_str!("../../abi/mvsystem/MobileVerifiersContractGameRoot.abi.json");

#[derive(Debug, Clone)]
pub struct MobileVerifiersGameRoot {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
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

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
impl AsyncGuarded<Account> for MobileVerifiersGameRoot {
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
impl AsyncGuardedMut<Account> for MobileVerifiersGameRoot {
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
    pub fn new(context: Arc<ClientContext>) -> Self {
        let address = "0:0505050505050505050505050505050505050505050505050505050505050505";
        Self {
            context: context.clone(),
            address: address.to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    /// # Get cell for boost contract to upgrade code
    ///
    /// Original contract method: `getCellForBoost`
    pub async fn get_cell_for_boost(
        &self,
        params: ParamsOfGetCellForBoost,
    ) -> anyhow::Result<ResultOfGetCellForBoost> {
        let call_set = CallSet {
            function_name: "getCellForBoost".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetCellForBoost>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }
}
