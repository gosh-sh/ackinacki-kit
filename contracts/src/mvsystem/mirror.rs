use std::collections::HashMap;
use std::sync::Arc;

use num_bigint::BigUint;
use num_traits::ToPrimitive;
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
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::error::MvSystemModule;
use crate::mvsystem::miner::contract::Miner;
use crate::mvsystem::PopitMedia;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::GetMethodAccessor;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/mvsystem/Mirror.abi.json");

#[derive(Debug, Clone)]
pub struct Mirror {
    context: Arc<ClientContext>,
    address: String,
    index: u128,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for Mirror {
    const MODULE: KitModule = KitModule::MvSystem(MvSystemModule::Mirror);
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

impl Executor for Mirror {}

impl SendMessage for Mirror {}

impl AsyncGuarded<Account> for Mirror {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Mirror {
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
    pub root_provider_certificates: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetMinerAddress {
    #[serde(rename(serialize = "multifactor"))]
    pub multifactor_address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetMinerAddress {
    #[serde(rename = "miner")]
    pub address: String,
}

impl Mirror {
    pub fn new(context: Arc<ClientContext>, public: impl AsRef<str>) -> KitResult<Self> {
        let public = {
            let bytes = hex::decode(public.as_ref()).map_err(|e| {
                KitError::new(
                    Self::MODULE,
                    KitErrorCode::Decode,
                    format!("Decode hex to bytes ({e})"),
                )
            })?;
            BigUint::from_bytes_be(&bytes)
        };

        let index = {
            let number = (public % BigUint::from(1000_u32)) + BigUint::from(1_u32);
            number.to_u64().map(|v| v as u128).ok_or_else(|| {
                KitError::new(
                    Self::MODULE,
                    KitErrorCode::Convert,
                    "Convert index to u64".to_string(),
                )
            })?
        };
        let address = format!("0:2{index:063x}");

        Ok(Self {
            context: context.clone(),
            address: address.clone(),
            index,
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        })
    }

    pub fn index(&self) -> u128 {
        self.index
    }

    /// # Get miner
    ///
    /// Original contract method: `getMinerAddress`
    pub async fn get_miner(&self, params: ParamsOfGetMinerAddress) -> KitResult<Miner> {
        let res_of_get_addr = self
            .call_get_method_with::<ResultOfGetMinerAddress, ParamsOfGetMinerAddress>(
                "getMinerAddress",
                params,
            )
            .await?;

        Ok(Miner::new(self.context.clone(), res_of_get_addr.address))
    }

    /// # Deploy multifactor account
    pub async fn deploy_multifactor(
        &self,
        params: ParamsOfDeployMultifactor,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployMultifactor".to_string(),
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
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployPopCoinRoot".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Encode deploy miner message
    ///
    /// Original contract method: `deployMiner`
    pub async fn deploy_miner_message(&self) -> KitResult<String> {
        let call_set =
            CallSet { function_name: "deployMiner".to_string(), header: None, input: None };

        let result = self.encode_message_body(call_set, true, Signer::None).await?;

        Ok(result.body)
    }
}
