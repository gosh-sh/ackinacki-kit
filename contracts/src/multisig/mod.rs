//! Binding for the `Multisig` contract — the standard multisig wallet users
//! deploy via `tvm-cli` on Acki Nacki networks.
//!
//! Scope is wallet operations callers need from the SDK:
//!
//! - `submit_transaction` — propose (and, when a single confirmation is
//!   required, immediately execute) an internal message to an arbitrary
//!   destination, carrying ECC currency and an ABI-encoded body cell.
//!   This is the path for routing calls like `RootPN.generateVoucher`
//!   through a user's wallet.
//! - `send_transaction` — direct send with explicit flags (single-custodian
//!   shortcut; bypasses the queued-confirmation flow).
//! - `confirm_transaction` — co-sign a queued transaction when
//!   `reqConfirms > 1`.
//! - Read methods (`get_parameters`, `get_custodians`, `get_transactions`,
//!   `get_transaction`, `get_transaction_ids`, `get_version`).
//!
//! Deploy is intentionally out of scope here — wallets are deployed by the
//! end-user via `tvm-cli` using the canonical ABI/TVC. This binding takes
//! an already-deployed wallet's address and drives it.

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

const ABI: &str = include_str!("../../abi/multisig/Multisig.abi.json");

/// Decoded persistent storage of the multisig wallet.
///
/// Field names mirror the contract ABI (`m_*` for state fields, `_*` for
/// the runtime preamble) via serde aliases so a single `Account` decode
/// hydrates the struct directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    #[serde(alias = "_pubkey")]
    pub pubkey: String,

    #[serde(alias = "_timestamp")]
    pub timestamp: String,

    #[serde(alias = "_constructorFlag")]
    pub constructor_flag: bool,

    #[serde(alias = "m_ownerKey")]
    pub owner_key: Option<String>,

    #[serde(alias = "m_ownerAddress")]
    pub owner_address: Option<String>,

    #[serde(alias = "m_requestsMask")]
    pub requests_mask: String,

    #[serde(alias = "m_requestsMaskData")]
    pub requests_mask_data: String,

    #[serde(alias = "m_custodianCount")]
    pub custodian_count: String,

    #[serde(alias = "m_defaultRequiredConfirmations")]
    pub default_required_confirmations: String,

    #[serde(alias = "m_defaultRequiredConfirmationsData")]
    pub default_required_confirmations_data: String,

    #[serde(alias = "_max_cleanup_operations")]
    pub max_cleanup_operations: String,
}

/// Custodian descriptor — either an external `owner_pubkey` (user signing
/// off-chain) or an on-chain `owner_address` (contract co-signer). Exactly
/// one of the two is populated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Custodian {
    pub owner_pubkey: Option<String>,
    pub owner_address: Option<String>,
    pub index: String,
}

/// Outstanding (queued or executed-this-block) transaction descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    #[serde(rename = "confirmationsMask")]
    pub confirmations_mask: String,
    #[serde(rename = "signsRequired")]
    pub signs_required: String,
    #[serde(rename = "signsReceived")]
    pub signs_received: String,
    pub creator: Custodian,
    pub dest: String,
    pub value: String,
    pub cc: HashMap<String, String>,
    #[serde(rename = "sendFlags")]
    pub send_flags: String,
    pub payload: String,
    pub bounce: bool,
    /// Destination dapp id stored with the queued transaction. Caller-supplied
    /// and retained for off-chain/API use; not consulted when the outbound
    /// message is sent.
    pub dapp_id: String,
}

#[derive(Debug, Clone)]
pub struct Multisig {
    context: Arc<ClientContext>,
    address: String,
    dapp_id: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for Multisig {
    const MODULE: KitModule = KitModule::Multisig;
}

impl AccountAccessor for Multisig {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Multisig {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Multisig {
    fn address(&self) -> &str {
        &self.address
    }

    fn dapp_id(&self) -> &str {
        &self.dapp_id
    }
}

impl ContextAccessor for Multisig {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl DecodeAccountData<AccountData> for Multisig {}

impl EncodeMessage for Multisig {}

impl DecodeMessage for Multisig {}

impl Executor for Multisig {}

impl SendMessage for Multisig {}

impl AsyncGuarded<Account> for Multisig {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Multisig {
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
pub struct ParamsOfSubmitTransaction {
    /// Destination contract address.
    pub dest: String,
    /// Native vmshell value attached to the internal message.
    pub value: u128,
    /// ECC currencies to attach: `currency_id → amount`.
    pub cc: HashMap<u32, u64>,
    pub bounce: bool,
    /// TVM send flags applied to the queued transaction when it executes.
    pub flag: u8,
    /// ABI-encoded body cell, base64-encoded BOC. Empty string for plain
    /// value transfers with no body.
    pub payload: String,
    /// Destination dapp id, as a uint256 decimal/hex string. Stored with the
    /// queued transaction for off-chain/API use; not consulted when the
    /// outbound message is sent. Defaults to `"0"`.
    pub dapp_id: String,
}

impl Default for ParamsOfSubmitTransaction {
    fn default() -> Self {
        Self {
            dest: Default::default(),
            value: 0,
            cc: Default::default(),
            bounce: true,
            flag: 1,
            payload: Default::default(),
            dapp_id: "0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfSubmitTransaction {
    #[serde(rename = "transId", deserialize_with = "deserialize_u64")]
    pub trans_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSendTransaction {
    pub dest: String,
    pub value: u128,
    pub cc: HashMap<u32, u64>,
    pub bounce: bool,
    /// TVM send flags. Note plural `flags` to match the ABI; differs from
    /// `submit_transaction`'s singular `flag`.
    pub flags: u8,
    pub payload: String,
    /// Destination dapp id, as a uint256 decimal/hex string. Stored with the
    /// queued transaction for off-chain/API use; not consulted when the
    /// outbound message is sent. Defaults to `"0"`.
    pub dapp_id: String,
}

impl Default for ParamsOfSendTransaction {
    fn default() -> Self {
        Self {
            dest: Default::default(),
            value: 0,
            cc: Default::default(),
            bounce: true,
            flags: 1,
            payload: Default::default(),
            dapp_id: "0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfConfirmTransaction {
    #[serde(rename = "transactionId")]
    pub transaction_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetTransaction {
    #[serde(rename = "transactionId")]
    pub transaction_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetTransaction {
    pub trans: Transaction,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetTransactions {
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetTransactionIds {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetCustodians {
    pub custodians: Vec<Custodian>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetParameters {
    #[serde(rename = "maxQueuedTransactions")]
    pub max_queued_transactions: String,
    #[serde(rename = "maxCustodianCount")]
    pub max_custodian_count: String,
    #[serde(rename = "expirationTime", deserialize_with = "deserialize_u64")]
    pub expiration_time: u64,
    #[serde(rename = "requiredTxnConfirms")]
    pub required_txn_confirms: String,
    #[serde(rename = "requiredDataConfirms")]
    pub required_data_confirms: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetVersion {
    #[serde(rename = "value0")]
    pub kind: String,
    #[serde(rename = "value1")]
    pub version: String,
}

impl Multisig {
    pub fn new(
        context: Arc<ClientContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        let params = params.into();
        Self {
            context: context.clone(),
            address: params.address.clone(),
            dapp_id: params.dapp_id.clone(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, &params.address, params.dapp_id))),
        }
    }

    /// # Submit transaction
    ///
    /// Original contract method: `submitTransaction`
    ///
    /// Propose an outbound transaction. When `reqConfirms == 1` and the
    /// submitter is a custodian, the wallet executes the transaction in
    /// the same block. Otherwise it queues for `confirm_transaction`.
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

    /// # Send transaction
    ///
    /// Original contract method: `sendTransaction`
    ///
    /// Direct transfer with explicit flags, bypassing the confirmation
    /// queue. Only valid for single-custodian wallets (or wallets
    /// configured to allow it).
    pub async fn send_transaction(
        &self,
        params: ParamsOfSendTransaction,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "sendTransaction".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Confirm transaction
    ///
    /// Original contract method: `confirmTransaction`
    ///
    /// Co-sign a queued transaction submitted earlier by another custodian.
    pub async fn confirm_transaction(
        &self,
        params: ParamsOfConfirmTransaction,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "confirmTransaction".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Get wallet parameters
    ///
    /// Original contract method: `getParameters`
    pub async fn get_parameters(&self) -> KitResult<ResultOfGetParameters> {
        self.call_get_method::<ResultOfGetParameters>("getParameters").await
    }

    /// # Get custodians
    ///
    /// Original contract method: `getCustodians`
    pub async fn get_custodians(&self) -> KitResult<ResultOfGetCustodians> {
        self.call_get_method::<ResultOfGetCustodians>("getCustodians").await
    }

    /// # Get all queued transactions
    ///
    /// Original contract method: `getTransactions`
    pub async fn get_transactions(&self) -> KitResult<ResultOfGetTransactions> {
        self.call_get_method::<ResultOfGetTransactions>("getTransactions").await
    }

    /// # Get one queued transaction
    ///
    /// Original contract method: `getTransaction`
    pub async fn get_transaction(
        &self,
        params: ParamsOfGetTransaction,
    ) -> KitResult<ResultOfGetTransaction> {
        self.call_get_method_with::<ResultOfGetTransaction, ParamsOfGetTransaction>(
            "getTransaction",
            params,
        )
        .await
    }

    /// # Get queued transaction ids
    ///
    /// Original contract method: `getTransactionIds`
    pub async fn get_transaction_ids(&self) -> KitResult<ResultOfGetTransactionIds> {
        self.call_get_method::<ResultOfGetTransactionIds>("getTransactionIds").await
    }

    /// # Get wallet kind/version
    ///
    /// Original contract method: `getVersion`
    pub async fn get_version(&self) -> KitResult<ResultOfGetVersion> {
        self.call_get_method::<ResultOfGetVersion>("getVersion").await
    }
}

#[cfg(test)]
mod tests {
    use super::ParamsOfSendTransaction;
    use super::ParamsOfSubmitTransaction;
    use super::ABI;

    /// Input-parameter names declared for `func` in the bundled ABI.
    fn abi_input_names(func: &str) -> Vec<String> {
        let abi: serde_json::Value = serde_json::from_str(ABI).expect("ABI is valid JSON");
        abi["functions"]
            .as_array()
            .expect("ABI.functions array")
            .iter()
            .find(|f| f["name"] == func)
            .unwrap_or_else(|| panic!("function `{func}` not found in ABI"))["inputs"]
            .as_array()
            .expect("function inputs array")
            .iter()
            .map(|i| i["name"].as_str().expect("input name").to_string())
            .collect()
    }

    /// Every ABI input of `func` must have a matching key in the serialized
    /// params struct. This guards the binding against drifting from the ABI —
    /// a renamed/missing field (`flag` vs `flags`, `dapp_id` vs `dappId`, the
    /// `dapp_id` addition, …) would only fail on-chain otherwise.
    fn assert_params_cover_abi(func: &str, serialized: &serde_json::Value) {
        for name in abi_input_names(func) {
            assert!(
                serialized.get(&name).is_some(),
                "ABI input `{name}` of `{func}` is missing from the serialized params \
                 — binding is out of sync with the ABI"
            );
        }
    }

    #[test]
    fn submit_transaction_params_match_abi() {
        let v = serde_json::to_value(ParamsOfSubmitTransaction::default())
            .expect("serialize submit params");
        assert_params_cover_abi("submitTransaction", &v);
    }

    #[test]
    fn send_transaction_params_match_abi() {
        let v = serde_json::to_value(ParamsOfSendTransaction::default())
            .expect("serialize send params");
        assert_params_cover_abi("sendTransaction", &v);
    }
}
