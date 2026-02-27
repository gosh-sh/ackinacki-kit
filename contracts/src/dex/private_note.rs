use std::collections::HashMap;
use std::sync::Arc;

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
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u128_map;
use crate::error::DexModule;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/dex/PrivateNote.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for the DEX `PrivateNote` contract.
pub struct PrivateNote {
    base: ContractBase,
}

impl ModuleAccessor for PrivateNote {
    const MODULE: KitModule = KitModule::Dex(DexModule::PrivateNote);
}

impl HasContractBase for PrivateNote {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for PrivateNote {}

impl AsyncGuarded<Account> for PrivateNote {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for PrivateNote {
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
/// Parameters for `PrivateNote.changeOwner`.
pub struct ParamsOfChangeOwner {
    #[serde(rename(serialize = "new_pubkey"))]
    pub new_pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.deployPMP`.
pub struct ParamsOfDeployPmp {
    pub event_id: String,
    #[serde(rename(serialize = "oracleFee"))]
    pub oracle_fee: Vec<u128>,
    pub token_type: u32,
    pub names: Vec<String>,
    pub index: Vec<u128>,
    #[serde(rename(serialize = "initialStakes"))]
    pub initial_stakes: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Shared PMP key (`event_id`, `oracle_list_hash`, `token_type`) used by
/// multiple `PrivateNote` methods (`deleteStake`, `cancelStake`, `claim`).
pub struct ParamsOfStakeKey {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.withdrawFullSet`.
pub struct ParamsOfWithdrawFullSet {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub amount: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.setStake`.
pub struct ParamsOfSetStake {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub outcome: u32,
    pub amount: u128,
    pub use_coupon: bool,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.setFullSetStake`.
pub struct ParamsOfSetFullSetStake {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub amount: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.generateCoupon`.
pub struct ParamsOfGenerateCoupon {
    pub token_type: u32,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.withdrawTokens`.
pub struct ParamsOfWithdrawTokens {
    pub flags: u8,
    pub dest_wallet_addr: String,
    pub token_type: u32,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PrivateNote.getPMPCode`.
pub struct ResultOfGetPmpCode {
    #[serde(rename = "pmpCode")]
    pub pmp_code: String,
    #[serde(rename = "pmpCodeHash")]
    pub pmp_code_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PrivateNote.getDetails`.
pub struct ResultOfGetDetails {
    #[serde(rename = "depositIdentifierHash")]
    pub deposit_identifier_hash: String,
    #[serde(rename = "etherealPubkey")]
    pub ethereal_pubkey: String,
    #[serde(deserialize_with = "deserialize_u128_map")]
    pub balance: HashMap<String, u128>,
    #[serde(rename = "pmpCodeHash")]
    pub pmp_code_hash: String,
    #[serde(rename = "privateNoteCodeHash")]
    pub private_note_code_hash: String,
    #[serde(rename = "busyAddress")]
    pub busy_address: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PrivateNote._deposit_identifier_hash`.
pub struct ResultOfGetDepositIdentifierHash {
    #[serde(rename = "_deposit_identifier_hash")]
    pub deposit_identifier_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PrivateNote._stakes`.
///
/// Stake entries are intentionally kept as raw JSON to keep the wrapper stable
/// across DEX stake tuple schema changes.
pub struct ResultOfGetStakes {
    #[serde(rename = "_stakes")]
    pub stakes: HashMap<String, serde_json::Value>,
}

impl PrivateNote {
    /// Create a wrapper for a deployed `PrivateNote`.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// # Change ephemeral owner key
    ///
    /// Original contract method: `changeOwner`
    ///
    /// Should be signed with current ephemeral owner keys
    pub async fn change_owner(
        &self,
        params: ParamsOfChangeOwner,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "changeOwner".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy PMP
    ///
    /// Original contract method: `deployPMP`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn deploy_pmp(
        &self,
        params: ParamsOfDeployPmp,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployPMP".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Delete stake record
    ///
    /// Original contract method: `deleteStake`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn delete_stake(
        &self,
        params: ParamsOfStakeKey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deleteStake".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Cancel stake on PMP
    ///
    /// Original contract method: `cancelStake`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn cancel_stake(
        &self,
        params: ParamsOfStakeKey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "cancelStake".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Withdraw full-set stake from PMP
    ///
    /// Original contract method: `withdrawFullSet`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn withdraw_full_set(
        &self,
        params: ParamsOfWithdrawFullSet,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "withdrawFullSet".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Place a single-outcome stake
    ///
    /// Original contract method: `setStake`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn set_stake(
        &self,
        params: ParamsOfSetStake,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setStake".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Place a full-set stake
    ///
    /// Original contract method: `setFullSetStake`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn set_full_set_stake(
        &self,
        params: ParamsOfSetFullSetStake,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setFullSetStake".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Claim PMP payout
    ///
    /// Original contract method: `claim`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn claim(
        &self,
        params: ParamsOfStakeKey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "claim".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Generate coupon
    ///
    /// Original contract method: `generateCoupon`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn generate_coupon(
        &self,
        params: ParamsOfGenerateCoupon,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "generateCoupon".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Withdraw tokens via RootPN vault
    ///
    /// Original contract method: `withdrawTokens`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn withdraw_tokens(
        &self,
        params: ParamsOfWithdrawTokens,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "withdrawTokens".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Get salted PMP code and hash
    ///
    /// Original contract method: `getPMPCode`
    pub async fn get_pmp_code(&self) -> KitResult<ResultOfGetPmpCode> {
        self.call_get_method::<ResultOfGetPmpCode>("getPMPCode").await
    }

    /// # Get PrivateNote details
    ///
    /// Original contract method: `getDetails`
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// # Get deposit identifier hash (public static field getter)
    ///
    /// Original contract method: `_deposit_identifier_hash`
    pub async fn get_deposit_identifier_hash(&self) -> KitResult<ResultOfGetDepositIdentifierHash> {
        self.call_get_method::<ResultOfGetDepositIdentifierHash>("_deposit_identifier_hash").await
    }

    /// # Get raw `_stakes` mapping
    ///
    /// Original contract method: `_stakes`
    ///
    /// Returns raw JSON entries because stake tuple schema is large and evolves
    /// frequently in DEX contract iterations.
    pub async fn get_stakes(&self) -> KitResult<ResultOfGetStakes> {
        self.call_get_method::<ResultOfGetStakes>("_stakes").await
    }
}
