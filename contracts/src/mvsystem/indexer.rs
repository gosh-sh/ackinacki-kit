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
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/Indexer.abi.json");

#[derive(Debug)]
pub struct Indexer {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for Indexer {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Indexer {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Indexer {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for Indexer {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for Indexer {}

impl DecodeMessage for Indexer {}

impl Executor for Indexer {}

impl SendMessage for Indexer {}

#[async_trait]
impl AsyncGuarded<Account> for Indexer {
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
impl AsyncGuardedMut<Account> for Indexer {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ResultOfGetDetails {
    pub name: String,
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfSetOwner {
    #[serde(rename(serialize = "wallet"))]
    pub multifactor_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamsOfDeployMultifactor {
    #[serde(rename(serialize = "wallet"))]
    pub multifactor_address: String,
    pub zkid: String,
    pub proof: String,
    pub epk: String,
    pub epk_sig: String,
    pub epk_expire_at: u64,
    pub jwk_modulus: String,
    pub kid: String,
    pub jwk_modulus_expire_at: u64,
    pub index_mod_4: u8,
    pub iss_base_64: String,
    pub provider: String,
    pub header_base_64: String,
    pub pub_recovery_key: String,
    pub pub_recovery_key_sig: String,
    pub jwk_update_key: String,
    pub jwk_update_key_sig: String,
    pub root_provider_certificates: HashMap<String, String>,
    pub owner_pubkey: String,
    pub mirror: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfEncodeSetOwner {
    #[serde(rename(serialize = "wallet"))]
    pub multifactor_address: String,
}

impl Indexer {
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

    /// # Encode set indexer owner message
    ///
    /// Original contract method: `setNewWallet`
    ///
    /// Should be sent from current owner multifactor
    pub async fn set_owner_message(
        &self,
        params: ParamsOfEncodeSetOwner,
    ) -> anyhow::Result<String> {
        let call_set = CallSet {
            function_name: "setNewWallet".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self
            .encode_message_body(call_set, true, Signer::None)
            .await
            .map_err(|e| anyhow!("Encode message body ({e})"))?;

        Ok(result.body)
    }

    /// # Deploy multifactor
    ///
    /// Used to create multifactor and claim current indexer
    ///
    /// Original contract method: `isOwnerRoot`
    ///
    /// Should be signed with root keys
    pub async fn deploy_multifactor(
        &self,
        params: ParamsOfDeployMultifactor,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "isOwnerRoot".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
