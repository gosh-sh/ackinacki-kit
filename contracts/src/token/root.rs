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
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u128;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/token/RootToken.abi.json");

#[derive(Debug, Clone)]
pub struct RootToken {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for RootToken {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for RootToken {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for RootToken {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for RootToken {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for RootToken {}

impl DecodeMessage for RootToken {}

impl Executor for RootToken {}

impl SendMessage for RootToken {}

#[async_trait]
impl AsyncGuarded<Account> for RootToken {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T + Send + 'async_trait,
        T: Send + 'async_trait,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

#[async_trait]
impl AsyncGuardedMut<Account> for RootToken {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut + Send + 'async_trait,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'async_trait,
        T: Send + 'async_trait,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetDetails {
    pub name: String,
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub decimals: u128,
    pub deployer: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub minted: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub burned: u128,
    #[serde(rename = "mintDisabled")]
    pub mint_disabled: bool,
    #[serde(rename = "ownerPubkey")]
    pub owner_pubkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetWalletAddress {
    #[serde(rename = "walletAddress")]
    pub wallet_address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeployWallet {
    #[serde(rename(serialize = "owner"))]
    pub owner_address: String,
}

impl RootToken {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    pub async fn get_details(&self) -> anyhow::Result<ResultOfGetDetails> {
        let call_set =
            CallSet { function_name: "getDetails".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetDetails>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    pub async fn get_wallet_address(&self) -> anyhow::Result<ResultOfGetWalletAddress> {
        let call_set =
            CallSet { function_name: "getWalletAddress".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetWalletAddress>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    pub async fn deploy_wallet(
        &self,
        params: ParamsOfDeployWallet,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployWallet".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
