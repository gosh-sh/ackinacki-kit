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
use crate::deserialize::deserialize_u64;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/Mvmultifactor.abi.json");

#[derive(Debug, Clone)]
pub struct MvMultifactor {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for MvMultifactor {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for MvMultifactor {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for MvMultifactor {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for MvMultifactor {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for MvMultifactor {}

impl DecodeMessage for MvMultifactor {}

impl Executor for MvMultifactor {}

impl SendMessage for MvMultifactor {}

#[async_trait]
impl AsyncGuarded<Account> for MvMultifactor {
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
impl AsyncGuardedMut<Account> for MvMultifactor {
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

#[derive(Debug, Serialize)]
pub struct ParamsOfAddZkpFactor {
    pub proof: String,
    pub epk: String,
    pub kid: String,
    pub header_base_64: String,
    pub epk_expire_at: i64,
}
#[derive(Debug, Deserialize)]
pub struct ResultOfAddZkpFactor {
    pub success: bool,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfUpdateZkId {
    pub zkid: String,
    pub proof: String,
    pub epk: String,
    pub epk_sig: String,
    pub epk_expire_at: i64,
    pub jwk_modulus: String,
    pub kid: String,
    pub jwk_modulus_expire_at: i64,
    pub index_mod_4: i64,
    pub iss_base_64: String,
    pub header_base_64: String,
    pub owner_pubkey: String,
    pub root_provider_certificates: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfSubmitTransaction {
    pub dest: String,
    pub value: u128,
    pub cc: HashMap<u32, u32>,
    pub bounce: bool,
    #[serde(rename(serialize = "allBalance"))]
    pub all_balance: bool,
    pub epk_expire_at: u64,
    pub payload: String,
}

impl Default for ParamsOfSubmitTransaction {
    fn default() -> Self {
        Self {
            dest: Default::default(),
            value: 100_000_000,
            cc: Default::default(),
            bounce: true,
            all_balance: false,
            epk_expire_at: Default::default(),
            payload: Default::default(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ParamsOfGetEpkExpire {
    pub epk: String,
}

#[derive(Debug, Deserialize)]
pub struct ResultOfGetEpkExpire {
    #[serde(rename = "value0", deserialize_with = "deserialize_u64")]
    pub epk_expire_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct ResultOfGetZkpEphemeralPublicKeys {
    #[serde(rename = "value0")]
    pub keys: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfAddJwkModulus {
    pub root_cert_sn: String,
    pub lv_kid: String,
    pub tls_data: String,
}

impl MvMultifactor {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    /// # Get expiration unixtime of provided ephemeral public key
    ///
    /// Original contract method: `get_epk_expire_at`
    pub async fn get_epk_expire_at(
        &self,
        params: ParamsOfGetEpkExpire,
    ) -> anyhow::Result<ResultOfGetEpkExpire> {
        let call_set = CallSet {
            function_name: "get_epk_expire_at".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetEpkExpire>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Get list of ephemeral public keys
    ///
    /// Original contract method: `getZKPEphemeralPublicKeys`
    pub async fn get_zkp_ephemeral_public_keys(
        &self,
    ) -> anyhow::Result<ResultOfGetZkpEphemeralPublicKeys> {
        let call_set = CallSet {
            function_name: "getZKPEphemeralPublicKeys".to_string(),
            header: None,
            input: None,
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetZkpEphemeralPublicKeys>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Update ZK id
    ///
    /// Original contract method: `updateZkid`
    ///
    /// Should be signed by any valid ephemeral keypair
    pub async fn update_zk_id(
        &self,
        params: ParamsOfUpdateZkId,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "updateZkid".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Add ZKP factor
    ///
    /// Original contract method: `addZKPfactor`
    ///
    /// Should be signed by any valid ephemeral keypair
    pub async fn add_zkp_factor(
        &self,
        params: ParamsOfAddZkpFactor,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "addZKPfactor".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Submit transaction
    ///
    /// Original contract method: `submitTransaction`
    ///
    /// Should be signed by any valid ephemeral keypair
    pub async fn submit_transaction(
        &self,
        params: ParamsOfSubmitTransaction,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "submitTransaction".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Add JWK modulus
    ///
    /// Original contract method: `addJwkModulus`
    ///
    /// Should be signed by any valid ephemeral keypair
    pub async fn add_jwk_modulus(
        &self,
        params: ParamsOfAddJwkModulus,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "addJwkModulus".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }
}
