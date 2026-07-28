//! Binding for `UpdateCustodianMultisigWallet_v2` — the only multisig build
//! the kit speaks to. The bundled ABI/TVC under `contracts/abi/multisig/` are
//! vendored verbatim from `gosh-sh/acki-nacki` (`dev`, commit `6ad89549`,
//! `contracts/0.81.0_compiled/updatecustodianmultisigwallet_v2/`); see
//! [`CODE_HASH`] and the `vendored_asset_is_pinned` test.
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
//! - `submit_update_code` / `confirm_update_code` — v2-only code upgrade,
//!   queued and confirmed like a transaction.
//! - Read methods (`get_parameters`, `get_custodians`, `get_transactions`,
//!   `get_transaction`, `get_transaction_ids`, `get_version`).
//!
//! # Reads go through storage, not get-methods
//!
//! Every read here decodes the account's data cell ([`Multisig::account_data`])
//! instead of executing a get-method. Running any get-method against the v2
//! code fails in `run_tvm` with
//!
//! ```text
//! code 404  TVM internal error: can not parse actions: 0
//! ```
//!
//! because `tvm_block`'s action-list parser does not recognise a tag emitted by
//! `sol 0.81.0` output (`tvm_block/src/out_actions.rs`; identical in tvm-sdk
//! 3.0.2 and 3.0.4, so bumping the SDK does not help). Verified on shellnet for
//! all six read methods, with the v2 ABI. Writes are unaffected — they go
//! through `process_message`, not `run_tvm`.
//!
//! Two consequences of reading storage:
//!
//! - `maxQueuedTransactions`, `maxCustodianCount`, `expirationTime` and the
//!   `getVersion` strings are compile-time constants of the contract and are
//!   not in the data cell. They are mirrored here as [`MAX_QUEUED_TRANSACTIONS`],
//!   [`MAX_CUSTODIAN_COUNT`], [`EXPIRATION_TIME`], [`VERSION`] and
//!   [`CONTRACT_NAME`], and must be re-checked whenever the assets are
//!   refreshed.
//! - Storage layout is build-specific. Wallets deployed from the older flat
//!   `Multisig` build still accept *writes* from this binding (all 17 shared
//!   functions keep their signatures and function ids), but decoding their
//!   storage with the v2 ABI is not valid.
//!
//! If `tvm_block` learns the action tag, the get-methods start working again on
//! their own and this indirection becomes optional.
//!
//! Deploy is intentionally out of scope here — wallets are deployed by the
//! end-user via `tvm-cli` using the canonical ABI/TVC. This binding takes
//! an already-deployed wallet's address and drives it. Note that on ABI ≥ 2.3
//! the deploy address depends on the ABI's `fields` list as well as the code,
//! so a deployer must pair this TVC with this ABI.

use std::collections::BTreeMap;
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
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeAccountData;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::ModuleAccessor;
use crate::traits::ResultOfGetVersion;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/multisig/Multisig.abi.json");

/// Code hash of the bundled TVC — the value a node reports for an account
/// deployed from it. Repr hash of the state-init code cell.
pub const CODE_HASH: &str = "09f596d5bb4f63d7f2b18020ee0b7c9e88114dc90010389cc594c67954655ded";

/// `MAX_QUEUED_REQUESTS` — per-custodian cap on unconfirmed requests.
/// Contract constant, not stored; mirrored from the vendored build.
pub const MAX_QUEUED_TRANSACTIONS: u8 = 5;

/// `MAX_CUSTODIAN_COUNT`. Contract constant, not stored.
pub const MAX_CUSTODIAN_COUNT: u8 = 32;

/// `EXPIRATION_TIME`, in seconds. Contract constant, not stored.
pub const EXPIRATION_TIME: u64 = 3601;

/// First return value of `getVersion`. Contract constant, not stored.
///
/// Note it reads `2.2.0` even though the asset directory is named `v2.1.0`
/// upstream — this is the string the deployed code returns.
pub const VERSION: &str = "2.2.0";

/// Second return value of `getVersion`. Contract constant, not stored.
pub const CONTRACT_NAME: &str = "UpdateCustodianMultisigWallet_v2";

/// Decoded persistent storage of the multisig wallet.
///
/// Mirrors the ABI's `fields` list one-to-one (`m_*` for state, `_*` for the
/// runtime preamble) via serde aliases, so a single `Account` decode hydrates
/// the struct directly. The maps are `BTreeMap` so iteration order is stable.
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

    /// v2 addition: per-custodian request mask for queued code updates.
    #[serde(alias = "m_requestsMaskCode")]
    pub requests_mask_code: String,

    /// Queued transactions, keyed by transaction id (decimal string).
    #[serde(alias = "m_transactions", default)]
    pub transactions: BTreeMap<String, Transaction>,

    /// Queued custodian/confirmation updates, keyed by request id.
    #[serde(alias = "m_data", default)]
    pub data_updates: BTreeMap<String, DataUpdate>,

    /// v2 addition: queued code updates, keyed by request id.
    #[serde(alias = "m_code", default)]
    pub code_updates: BTreeMap<String, CodeUpdate>,

    /// Custodians, keyed by `owner_pubkey` (`0x` + 64 hex).
    #[serde(alias = "m_custodians", default)]
    pub custodians: BTreeMap<String, Custodian>,

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

/// Queued custodian-set / required-confirmations update (`submitDataUpdate`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataUpdate {
    pub id: String,
    #[serde(rename = "confirmationsMask")]
    pub confirmations_mask: String,
    #[serde(rename = "signsRequired")]
    pub signs_required: String,
    #[serde(rename = "signsReceived")]
    pub signs_received: String,
    pub creator: Custodian,
    pub owners_pubkey: Vec<String>,
    pub owners_address: Vec<String>,
    #[serde(rename = "reqConfirms")]
    pub req_confirms: String,
    #[serde(rename = "reqConfirmsData")]
    pub req_confirms_data: String,
}

/// Queued code update (`submitUpdateCode`), v2 only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeUpdate {
    pub id: String,
    #[serde(rename = "confirmationsMask")]
    pub confirmations_mask: String,
    #[serde(rename = "signsRequired")]
    pub signs_required: String,
    #[serde(rename = "signsReceived")]
    pub signs_received: String,
    pub creator: Custodian,
    /// New code cell, base64-encoded BOC.
    pub newcode: String,
    /// Auxiliary cell handed to the upgrade. Named `cell` in the ABI.
    pub cell: String,
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

/// Parameters of `submitUpdateCode`. v2 only.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ParamsOfSubmitUpdateCode {
    /// New contract code, base64-encoded BOC of the code cell.
    pub newcode: String,
    /// Auxiliary cell handed to the upgrade, base64-encoded BOC. The ABI
    /// input is literally named `cell`.
    pub cell: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfSubmitUpdateCode {
    #[serde(rename = "codeUpdateId", deserialize_with = "deserialize_u64")]
    pub code_update_id: u64,
}

/// Parameters of `confirmUpdateCode`. v2 only.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ParamsOfConfirmUpdateCode {
    #[serde(rename = "codeUpdateId")]
    pub code_update_id: u64,
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

    /// # Submit code update
    ///
    /// Original contract method: `submitUpdateCode`
    ///
    /// Queue a code upgrade. Confirmed with `confirm_update_code`; applied
    /// once `reqConfirms` custodians have signed.
    pub async fn submit_update_code(
        &self,
        params: ParamsOfSubmitUpdateCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "submitUpdateCode".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Confirm code update
    ///
    /// Original contract method: `confirmUpdateCode`
    pub async fn confirm_update_code(
        &self,
        params: ParamsOfConfirmUpdateCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "confirmUpdateCode".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Fetches the account and decodes its persistent storage.
    ///
    /// This is the single network round-trip behind every read method; call it
    /// directly when you need more than one of them and want one fetch.
    pub async fn account_data(&self) -> KitResult<AccountData> {
        self.fetch_account().await?;

        let (deployed, data) =
            self.async_guarded(|account| (account.is_deployed(), account.data.clone())).await;

        if !deployed {
            return Err(KitError::new(
                Self::MODULE,
                KitErrorCode::AccountIsNotActive,
                format!("Account `{}` is not active", self.address()),
            ));
        }

        let data = data.ok_or_else(|| {
            KitError::new(
                Self::MODULE,
                KitErrorCode::EmptyData,
                format!("Account `{}` has no data cell", self.address()),
            )
        })?;

        self.decode_account_data(data)
    }

    /// # Get wallet parameters
    ///
    /// Original contract method: `getParameters`
    ///
    /// `requiredTxnConfirms` / `requiredDataConfirms` come from storage; the
    /// other three are contract constants (see the module docs).
    pub async fn get_parameters(&self) -> KitResult<ResultOfGetParameters> {
        let data = self.account_data().await?;
        Ok(ResultOfGetParameters {
            max_queued_transactions: MAX_QUEUED_TRANSACTIONS.to_string(),
            max_custodian_count: MAX_CUSTODIAN_COUNT.to_string(),
            expiration_time: EXPIRATION_TIME,
            required_txn_confirms: data.default_required_confirmations,
            required_data_confirms: data.default_required_confirmations_data,
        })
    }

    /// # Get custodians
    ///
    /// Original contract method: `getCustodians`
    ///
    /// Ordered by `owner_pubkey`, matching the on-chain dictionary iteration
    /// order (keys are zero-padded 64-hex, so lexicographic == numeric).
    pub async fn get_custodians(&self) -> KitResult<ResultOfGetCustodians> {
        let data = self.account_data().await?;
        Ok(ResultOfGetCustodians { custodians: data.custodians.into_values().collect() })
    }

    /// # Get all queued transactions
    ///
    /// Original contract method: `getTransactions`
    pub async fn get_transactions(&self) -> KitResult<ResultOfGetTransactions> {
        let data = self.account_data().await?;
        let mut transactions: Vec<Transaction> = data.transactions.into_values().collect();
        transactions.sort_by(|a, b| numeric_id_cmp(&a.id, &b.id));
        Ok(ResultOfGetTransactions { transactions })
    }

    /// # Get one queued transaction
    ///
    /// Original contract method: `getTransaction`
    ///
    /// Errors when the id is not queued — the contract throws `102` in the
    /// same case.
    pub async fn get_transaction(
        &self,
        params: ParamsOfGetTransaction,
    ) -> KitResult<ResultOfGetTransaction> {
        let mut data = self.account_data().await?;
        let trans =
            data.transactions.remove(&params.transaction_id.to_string()).ok_or_else(|| {
                KitError::new(
                    Self::MODULE,
                    KitErrorCode::EmptyResult,
                    format!(
                        "Transaction `{}` is not queued on `{}`",
                        params.transaction_id,
                        self.address()
                    ),
                )
            })?;
        Ok(ResultOfGetTransaction { trans })
    }

    /// # Get queued transaction ids
    ///
    /// Original contract method: `getTransactionIds`
    pub async fn get_transaction_ids(&self) -> KitResult<ResultOfGetTransactionIds> {
        let data = self.account_data().await?;
        let mut ids: Vec<String> = data.transactions.into_keys().collect();
        ids.sort_by(|a, b| numeric_id_cmp(a, b));
        Ok(ResultOfGetTransactionIds { ids })
    }

    /// # Get wallet kind/version
    ///
    /// Original contract method: `getVersion`
    ///
    /// Both strings are compile-time constants of the vendored build, so this
    /// answers from [`VERSION`] / [`CONTRACT_NAME`] without touching the
    /// network. It describes the bundled assets, not the code actually
    /// deployed at [`Multisig::address`] — compare the account's code hash
    /// against [`CODE_HASH`] if you need to confirm the two agree.
    pub fn get_version(&self) -> ResultOfGetVersion {
        ResultOfGetVersion {
            version: VERSION.to_string(),
            contract_name: CONTRACT_NAME.to_string(),
        }
    }
}

/// Orders unsigned decimal ids numerically. Map keys arrive as decimal strings
/// without leading zeros, so "shorter first, then lexicographic" is exact and
/// cannot fail the way parsing can.
fn numeric_id_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use base64::prelude::BASE64_STANDARD;
    use base64::Engine;
    use sha2::Digest;
    use sha2::Sha256;
    use tvm_client::boc::ParamsOfDecodeStateInit;

    use super::numeric_id_cmp;
    use super::AccountData;
    use super::ParamsOfConfirmTransaction;
    use super::ParamsOfConfirmUpdateCode;
    use super::ParamsOfGetTransaction;
    use super::ParamsOfSendTransaction;
    use super::ParamsOfSubmitTransaction;
    use super::ParamsOfSubmitUpdateCode;
    use super::ABI;
    use super::CODE_HASH;
    use crate::tests::create_context;

    const TVC: &[u8] = include_bytes!("../../abi/multisig/Multisig.tvc");

    /// sha256 of the vendored TVC, as recorded in bee-engine's
    /// `bee_wallet/assets/multisig/PROVENANCE.md`.
    const TVC_SHA256: &str = "535e180e85ee019c23631c6046449fa2a5536d88f55b26d64e026d671e82d520";

    fn abi_json() -> serde_json::Value {
        serde_json::from_str(ABI).expect("ABI is valid JSON")
    }

    /// Input-parameter names declared for `func` in the bundled ABI.
    fn abi_input_names(func: &str) -> Vec<String> {
        abi_json()["functions"]
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

    #[test]
    fn confirm_transaction_params_match_abi() {
        let v = serde_json::to_value(ParamsOfConfirmTransaction { transaction_id: 1 })
            .expect("serialize confirm params");
        assert_params_cover_abi("confirmTransaction", &v);
    }

    #[test]
    fn get_transaction_params_match_abi() {
        let v = serde_json::to_value(ParamsOfGetTransaction { transaction_id: 1 })
            .expect("serialize get params");
        assert_params_cover_abi("getTransaction", &v);
    }

    #[test]
    fn submit_update_code_params_match_abi() {
        let v = serde_json::to_value(ParamsOfSubmitUpdateCode::default())
            .expect("serialize submit update code params");
        assert_params_cover_abi("submitUpdateCode", &v);
    }

    #[test]
    fn confirm_update_code_params_match_abi() {
        let v = serde_json::to_value(ParamsOfConfirmUpdateCode::default())
            .expect("serialize confirm update code params");
        assert_params_cover_abi("confirmUpdateCode", &v);
    }

    /// The bundled ABI must be the v2 build, not the older flat `Multisig`:
    /// the two share all 17 other functions, so only the code-update pair and
    /// the two extra storage fields tell them apart.
    #[test]
    fn bundled_abi_is_update_custodian_v2() {
        let abi = abi_json();

        let functions: Vec<&str> = abi["functions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["name"].as_str().unwrap())
            .collect();
        for name in ["submitUpdateCode", "confirmUpdateCode"] {
            assert!(functions.contains(&name), "ABI is missing v2 function `{name}`");
        }

        let fields: Vec<&str> =
            abi["fields"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
        for name in ["m_requestsMaskCode", "m_code"] {
            assert!(fields.contains(&name), "ABI is missing v2 storage field `{name}`");
        }
    }

    /// Pins the vendored TVC: content hash, and the code hash a node reports
    /// for accounts deployed from it. A swapped asset fails here rather than
    /// on a network.
    #[test]
    fn vendored_asset_is_pinned() {
        assert_eq!(hex::encode(Sha256::digest(TVC)), TVC_SHA256, "Multisig.tvc content changed");

        let decoded = tvm_client::boc::decode_state_init(
            create_context(),
            ParamsOfDecodeStateInit { state_init: BASE64_STANDARD.encode(TVC), boc_cache: None },
        )
        .expect("decode state init");

        assert_eq!(decoded.code_hash.as_deref(), Some(CODE_HASH));
        assert_eq!(decoded.compiler_version.as_deref(), Some("sol 0.81.0"));
    }

    /// Decodes the TVC's initial data cell with the bundled ABI. Proves
    /// [`AccountData`] matches the ABI's `fields` layout without needing a
    /// network — the read methods are only as good as this decode.
    #[test]
    fn account_data_matches_abi_layout() {
        let context = create_context();

        let data = tvm_client::boc::decode_state_init(
            context.clone(),
            ParamsOfDecodeStateInit { state_init: BASE64_STANDARD.encode(TVC), boc_cache: None },
        )
        .expect("decode state init")
        .data
        .expect("state init carries a data cell");

        let decoded = tvm_client::abi::decode_account_data(
            context,
            tvm_client::abi::ParamsOfDecodeAccountData {
                abi: tvm_client::abi::Abi::Json(ABI.to_string()),
                data,
                allow_partial: true,
            },
        )
        .expect("decode account data")
        .data;

        let account_data: AccountData =
            serde_json::from_value(decoded.clone()).unwrap_or_else(|e| {
                panic!("AccountData does not match the ABI layout ({e}): {decoded}")
            });

        // Pre-constructor state init: no custodians, nothing queued.
        assert!(account_data.custodians.is_empty());
        assert!(account_data.transactions.is_empty());
        assert!(account_data.code_updates.is_empty());
    }

    #[test]
    fn transaction_ids_sort_numerically() {
        let mut ids = ["10".to_string(), "9".to_string(), "100".to_string(), "11".to_string()];
        ids.sort_by(|a, b| numeric_id_cmp(a, b));
        assert_eq!(ids, ["9", "10", "11", "100"]);
    }
}
