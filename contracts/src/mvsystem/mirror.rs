use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use num_bigint::BigUint;
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
use crate::mvsystem::PopitMedia;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::EncodeMessage;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/Mirror.abi.json");

#[derive(Debug)]
pub struct Mirror {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for Mirror {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Mirror {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Mirror {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for Mirror {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for Mirror {}

impl SendMessage for Mirror {}

#[async_trait]
impl AsyncGuarded<Account> for Mirror {
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
impl AsyncGuardedMut<Account> for Mirror {
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
pub struct ParamsOfDeployMultifactor {
    pub name: String,
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
    pub owner_pubkey: String,
    pub root_provider_certificates: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfDeployPopitgame {
    #[serde(rename(serialize = "multifactor"))]
    pub multifactor_address: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfDeployPopcoinRoot {
    pub name: String,
    #[serde(rename(serialize = "maxPopitIndex"))]
    pub max_popit_index: u16,
    pub popits_media: HashMap<u16, PopitMedia>,
    #[serde(rename(serialize = "isPublic"))]
    pub is_public: bool,
    pub description: String,
    #[serde(rename(serialize = "popitGameOwner"))]
    pub owner_popitgame_address: String,
}

impl Mirror {
    pub fn new(context: Arc<ClientContext>, public: impl AsRef<str>) -> anyhow::Result<Self> {
        let public = {
            let bytes =
                hex::decode(public.as_ref()).map_err(|e| anyhow!("Decode hex to bytes ({e})"))?;
            BigUint::from_bytes_be(&bytes)
        };

        let address = {
            let index = (public % BigUint::from(1000_u32)) + BigUint::from(1_u32);
            format!("0:2{index:063x}")
        };

        Ok(Self {
            context: context.clone(),
            address: address.clone(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        })
    }

    /// # Deploy multifactor account
    pub async fn deploy_multifactor(
        &self,
        params: ParamsOfDeployMultifactor,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployMultifactor".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy popitgame account
    pub async fn deploy_popitgame(
        &self,
        params: ParamsOfDeployPopitgame,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployPopitGame".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy popcoin root account
    pub async fn deploy_popcoin_root(
        &self,
        params: ParamsOfDeployPopcoinRoot,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployPopCoinRoot".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
