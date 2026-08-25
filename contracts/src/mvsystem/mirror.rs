use std::collections::HashMap;

use num_bigint::BigUint;
use num_traits::ToPrimitive;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;

use crate::account::Account;
use crate::delivery::ContractContext;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::error::MvSystemModule;
use crate::mvsystem::miner::contract::Miner;
use crate::mvsystem::PopitMedia;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::AutoContract;
use crate::traits::ContextAccessor;
use crate::traits::ContractBase;
use crate::traits::EncodeMessage;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/mvsystem/Mirror.abi.json");

#[derive(Debug, Clone)]
pub struct Mirror {
    base: ContractBase,
    index: u128,
}

impl ModuleAccessor for Mirror {
    const MODULE: KitModule = KitModule::MvSystem(MvSystemModule::Mirror);
}

impl HasContractBase for Mirror {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for Mirror {}

impl AsyncGuarded<Account> for Mirror {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Mirror {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
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
    pub fn new(
        context: impl Into<ContractContext>,
        public: impl AsRef<str>,
        dapp_id: impl Into<String>,
    ) -> KitResult<Self> {
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

        let params = crate::account::ParamsOfNewContract::new(address, dapp_id);
        Ok(Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())), index })
    }

    /// Wrapper for the mirror of `public`, under the Mobile Verifiers dApp.
    pub fn new_default(
        context: impl Into<ContractContext>,
        public: impl AsRef<str>,
    ) -> KitResult<Self> {
        Self::new(context, public, crate::dapp::SystemDapp::MobileVerifiers)
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

        Ok(Miner::new(
            self.contract_context(),
            crate::account::ParamsOfNewContract::new(res_of_get_addr.address, self.dapp_id()),
        ))
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
