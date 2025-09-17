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
use crate::deserialize::deserialize_u32;
use crate::deserialize::deserialize_u64;
use crate::mvsystem::popcoin_wallet::PopcoinWallet;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/PopitGame.abi.json");

#[derive(Debug, Clone)]
pub struct Popitgame {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for Popitgame {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Popitgame {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Popitgame {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for Popitgame {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for Popitgame {}

impl DecodeMessage for Popitgame {}

impl Executor for Popitgame {}

impl SendMessage for Popitgame {}

#[async_trait]
impl AsyncGuarded<Account> for Popitgame {
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
impl AsyncGuardedMut<Account> for Popitgame {
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

#[derive(Debug, Deserialize)]
pub struct ResultOfGetDetails {
    #[serde(rename = "owner")]
    pub multifactor_address: String,
    #[serde(rename = "boost")]
    pub boosts_address: String,
    #[serde(rename = "root")]
    pub mobile_verifiers_root_address: String,
    #[serde(rename = "startTime", deserialize_with = "deserialize_u32")]
    pub start_time: u32,
    #[serde(rename = "mbiCur", deserialize_with = "deserialize_u64")]
    pub mamaboard_max_seq_no: u64,
    #[serde(deserialize_with = "deserialize_u128")]
    pub rewards: u128,
    #[serde(rename = "minstake", deserialize_with = "deserialize_u128")]
    pub min_stake: u128,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfGetPopcoinWallet {
    #[serde(rename = "name")]
    pub token_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ResultOfGetPopcoinWalletAddress {
    #[serde(rename = "popCoinWalletAddress")]
    pub address: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfDeployPopcoinWallet {
    #[serde(rename = "name")]
    pub token_name: String,
    #[serde(rename = "value")]
    pub amount: u128,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfEncodeWithdraw {
    #[serde(rename = "to")]
    pub recipient: String,
    #[serde(rename = "value")]
    pub amount: u128,
}

impl Popitgame {
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

    /// # Encode withdraw message
    ///
    /// Original contract method: `withdraw`
    ///
    /// Should be sent from owner multifactor contract
    pub async fn withdraw_message(&self, params: ParamsOfEncodeWithdraw) -> anyhow::Result<String> {
        let call_set = CallSet {
            function_name: "withdraw".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self
            .encode_message_body(call_set, true, Signer::None)
            .await
            .map_err(|e| anyhow!("Encode message body ({e})"))?;

        Ok(result.body)
    }

    /// # Get popcoin wallet instance
    ///
    /// Original contract method: `getPopCoinWalletAddress`
    pub async fn get_popcoin_wallet(
        &self,
        params: ParamsOfGetPopcoinWallet,
    ) -> anyhow::Result<PopcoinWallet> {
        let call_set = CallSet {
            function_name: "getPopCoinWalletAddress".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => {
                    let decoded = serde_json::from_value::<ResultOfGetPopcoinWalletAddress>(value)
                        .map_err(|e| anyhow!("Deserialize output ({e})"))?;
                    Ok(PopcoinWallet::new(self.context().clone(), decoded.address))
                }
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Deploy popcoin wallet
    ///
    /// Original contract method: `deployPopCoinWallet`
    ///
    /// Should be signed with server keys
    pub async fn deploy_popcoin_wallet(
        &self,
        params: ParamsOfDeployPopcoinWallet,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployPopCoinWallet".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
