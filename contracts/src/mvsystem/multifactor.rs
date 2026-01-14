use std::collections::HashMap;
use std::sync::Arc;

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
use crate::error::KitModule;
use crate::error::MvSystemModule;
use crate::mvsystem::ContractIndex;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeAccountData;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::GetMethodAccessor;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/mvsystem/Mvmultifactor.abi.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    #[serde(alias = "_candidate_new_owner_pubkey_and_expiration")]
    pub candidate_new_owner_pubkey_and_expiration: Option<HashMap<String, String>>,

    #[serde(alias = "_factors_len")]
    pub factors_len: String,

    #[serde(alias = "_factors_ordered_by_timestamp")]
    pub factors_ordered_by_timestamp: HashMap<String, String>,

    #[serde(alias = "_force_remove_oldest")]
    pub force_remove_oldest: bool,

    #[serde(alias = "_index_mod_4")]
    pub index_mod_4: String,

    #[serde(alias = "_iss_base_64")]
    pub iss_base_64: String,

    #[serde(alias = "_jwk_modulus_data")]
    pub jwk_modulus_data: HashMap<String, JwkData>,

    #[serde(alias = "_jwk_modulus_data_len")]
    pub jwk_modulus_data_len: String,

    #[serde(alias = "_m_security_cards_len")]
    pub m_security_cards_len: String,

    #[serde(alias = "_m_transactions_len")]
    pub m_transactions_len: String,

    #[serde(alias = "_max_cleanup_txns")]
    pub max_cleanup_txns: String,

    #[serde(alias = "_min_value")]
    pub min_value: String,

    #[serde(alias = "_name")]
    pub name: String,

    #[serde(alias = "_owner_pubkey")]
    pub owner_pubkey: String,

    #[serde(alias = "_pub_recovery_key")]
    pub pub_recovery_key: String,

    #[serde(alias = "_root")]
    pub root: String,

    #[serde(alias = "_use_security_card")]
    pub use_security_card: bool,

    #[serde(alias = "_zkid")]
    pub zkid: String,

    #[serde(alias = "_jwk_update_key")]
    pub jwk_update_key: String,

    #[serde(alias = "_wasm_hash")]
    pub wasm_hash: String,

    #[serde(alias = "_whiteListOfAddress")]
    pub white_list_of_address: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwkData {
    pub modulus: String,
    pub modulus_expire_at: String,
}

#[derive(Debug, Clone)]
pub struct Multifactor {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for Multifactor {
    const MODULE: KitModule = KitModule::MvSystem(MvSystemModule::Multifactor);
}

impl AccountAccessor for Multifactor {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Multifactor {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Multifactor {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for Multifactor {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl DecodeAccountData<AccountData> for Multifactor {}

impl EncodeMessage for Multifactor {}

impl DecodeMessage for Multifactor {}

impl Executor for Multifactor {}

impl SendMessage for Multifactor {}

impl AsyncGuarded<Account> for Multifactor {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Multifactor {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfAddZkpFactor {
    pub proof: String,
    pub epk: String,
    pub kid: String,
    pub header_base_64: String,
    pub epk_expire_at: i64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfAddZkpFactor {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize)]
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
    pub jwk_update_key: String,
    pub jwk_update_key_sig: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSubmitTransaction {
    pub dest: String,
    pub value: u128,
    pub cc: HashMap<u32, u64>,
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

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetEpkExpire {
    pub epk: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetEpkExpire {
    #[serde(rename = "value0", deserialize_with = "deserialize_u64")]
    pub epk_expire_at: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetZkpEphemeralPublicKeys {
    #[serde(rename = "value0")]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfAddJwkModulus {
    pub root_cert_sn: String,
    pub lv_kid: String,
    pub tls_data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfChangeSeedPhrase {
    pub epk_expire_at: u64,
    pub new_owner_pubkey: String,
    pub new_owner_pubkey_sig: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfAcceptCandidateSeedPhrase {
    pub new_owner_pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeleteCandidateSeedPhrase {
    pub epk_expire_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfUpdateRecoveryPhrase {
    pub new_pub_recovery_key: String,
    pub new_pub_recovery_key_sig: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeleteZKPFactorByItself {
    pub epk_expire_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfUpdateJwkUpdateKey {
    pub new_jwk_update_key: String,
    pub new_jwk_update_key_sig: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeleteJwkModulusByFactor {
    pub epk_expire_at: u64,
    pub kid: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfUpdateWhitelist {
    pub epk_expire_at: u64,
    /// Destination contract enum
    #[serde(rename(serialize = "index"))]
    pub payload_destination: ContractIndex,
    /// Popcoin name in case of destination contract PopCoin, empty string in other cases
    pub name: String,
    /// Any random value 0..999
    #[serde(rename(serialize = "indexMirror"))]
    pub mirror_index: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfCleanWhitelist {
    pub epk_expire_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetForceRemoveOldest {
    pub flag: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetWasmHash {
    pub wasm_hash: String,
}

impl Multifactor {
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
    ) -> KitResult<ResultOfGetEpkExpire> {
        self.call_get_method_with::<ResultOfGetEpkExpire, ParamsOfGetEpkExpire>(
            "get_epk_expire_at",
            params,
        )
        .await
    }

    /// # Get list of ephemeral public keys
    ///
    /// Original contract method: `getZKPEphemeralPublicKeys`
    pub async fn get_zkp_ephemeral_public_keys(
        &self,
    ) -> KitResult<ResultOfGetZkpEphemeralPublicKeys> {
        self.call_get_method::<ResultOfGetZkpEphemeralPublicKeys>("getZKPEphemeralPublicKeys").await
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
    ) -> KitResult<ResultOfSendMessage> {
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
    ) -> KitResult<ResultOfSendMessage> {
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
    ) -> KitResult<ResultOfSendMessage> {
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
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "addJwkModulus".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Change seed phrase
    ///
    /// Original contract method: `changeSeedPhrase`
    ///
    /// Should be signed by ephemeral keys
    pub async fn change_seed_phrase(
        &self,
        params: ParamsOfChangeSeedPhrase,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "changeSeedPhrase".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Accept candidate seed phrase
    ///
    /// Original contract method: `acceptCandidateSeedPhrase`
    ///
    /// Should be signed by recovery keys
    pub async fn accept_candidate_seed_phrase(
        &self,
        params: ParamsOfAcceptCandidateSeedPhrase,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "acceptCandidateSeedPhrase".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Delete candidate seed phrase
    ///
    /// Original contract method: `deleteCandidateSeedPhrase`
    ///
    /// Should be signed by ephemeral keys
    pub async fn delete_candidate_seed_phrase(
        &self,
        params: ParamsOfDeleteCandidateSeedPhrase,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deleteCandidateSeedPhrase".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Update recovery seed phrase
    ///
    /// Original contract method: `updateRecoveryPhrase`
    ///
    /// Should be signed by owner keys
    pub async fn update_recovery_phrase(
        &self,
        params: ParamsOfUpdateRecoveryPhrase,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "updateRecoveryPhrase".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Delete ZKP factor by itself
    ///
    /// Original contract method: `deleteZKPfactorByItself`
    ///
    /// Should be signed by ephemeral keys
    pub async fn delete_zkp_factor_by_itself(
        &self,
        params: ParamsOfDeleteZKPFactorByItself,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deleteZKPfactorByItself".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Update JWK update key
    ///
    /// Original contract method: `updateJwkUpdateKey`
    ///
    /// Should be signed by owner keys
    pub async fn update_jwk_update_key(
        &self,
        params: ParamsOfUpdateJwkUpdateKey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "updateJwkUpdateKey".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Delete JWK modulus by factor
    ///
    /// Original contract method: `deleteJwkModulusByFactor`
    ///
    /// Should be signed by ephemeral keys
    pub async fn delete_jwk_modulus_by_factor(
        &self,
        params: ParamsOfDeleteJwkModulusByFactor,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deleteJwkModulusByFactor".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Update payload destination whitelist
    ///
    /// Original contract method: `updateWhiteList`
    ///
    /// Should be signed by any valid ephemeral keypair
    pub async fn update_whitelist(
        &self,
        params: ParamsOfUpdateWhitelist,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "updateWhiteList".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Clean destination payload whitelist
    ///
    /// Original contract method: `cleanWhiteList`
    ///
    /// Should be signed by any valid ephemeral keypair
    pub async fn clean_whitelist(
        &self,
        params: ParamsOfCleanWhitelist,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "cleanWhiteList".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Set _force_remove_oldest flag
    ///
    /// Original contract method: `setForceRemoveOldest`
    ///
    /// Should be signed by owner keys
    pub async fn set_force_remove_oldest(
        &self,
        params: ParamsOfSetForceRemoveOldest,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setForceRemoveOldest".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }

    /// # Set _wasm_hash value
    ///
    /// Original contract method: `setWasmHash`
    ///
    /// Should be signed by owner keys
    pub async fn set_wasm_hash(
        &self,
        params: ParamsOfSetWasmHash,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setWasmHash".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        self.send_message(Some(call_set), None, signer).await
    }
}

#[cfg(test)]
mod tests {
    use shared::traits::guarded::AsyncGuarded;

    use crate::mvsystem::multifactor::Multifactor;
    use crate::tests::create_context;
    use crate::traits::AccountAccessor;
    use crate::traits::DecodeAccountData;

    #[tokio::test]
    async fn test_decode_account_data() {
        let context = create_context();

        let multifactor = Multifactor::new(
            context,
            "0:372e7644281159ef3df9c7e06e5a247ea889986868c63909f069efc2a5250129",
        );
        let fetch = multifactor.fetch_account().await;
        assert!(fetch.is_ok());

        let data = multifactor.async_guarded(|account| account.data.clone()).await.unwrap();
        let decoded = multifactor
            .decode_account_data(data)
            .inspect_err(|e| eprintln!("Decode multifactor data ({e:?})"))
            .unwrap();
        assert_eq!(decoded.index_mod_4, "1");
    }
}
