//! Binding for `UpdateCustodianMultisigWallet_v2` — the only multisig build
//! the kit speaks to. The bundled ABI/TVC under `contracts/abi/multisig/` are
//! vendored verbatim from `gosh-sh/acki-nacki` (`dev`, commit `44fe02ea`,
//! `contracts/0.81.0_compiled/updatecustodianmultisigwallet_v2/`); see
//! [`CODE_HASH`] and the `vendored_asset_is_pinned` test.
//!
//! The vendored build is `v2.4.0` ([`VERSION`]). Nothing older is supported:
//! the wallet gained a fourth request queue and a stored balance config, so the
//! storage layout, the constructor and the event set all differ from earlier
//! builds. Writes signed for one build are not portable to another either — a
//! wallet whose code hash is not [`CODE_HASH`] should be driven by whatever
//! binding matches it, not by this one.
//!
//! The binding covers the whole contract surface:
//!
//! - Transfers — `submit_transaction` (propose, and execute in the same block
//!   when a single confirmation is required), `send_transaction` (direct send
//!   with explicit flags, single-custodian wallets only) and
//!   `confirm_transaction`. This is the path for routing a call through a
//!   user's wallet: the ABI-encoded body travels in `payload`.
//! - Custodian-set updates — `submit_data_update` / `confirm_data_update`.
//!   Applying one clears *all four* queues (see `RequestsDropped` upstream),
//!   so anything pending has to be submitted again under the new set.
//! - Code upgrades — `submit_update_code` / `confirm_update_code`, queued and
//!   confirmed like a transaction.
//! - Balance config — `submit_config_update` / `confirm_config_update`. The
//!   wallet tops its own vmshell (gas) balance up from SHELL when it drops
//!   below `minBalance`, converting up to `targetBalance`; `minBalance == 0`
//!   disables that. The config survives custodian changes.
//! - Cleanup budget — `set_max_cleanup_operations` (expired requests removed
//!   per pass, `>= `[`MIN_CLEANUP_OPERATIONS`]).
//! - Reads for each of the four queues (by id, as a list, as a list of ids)
//!   plus `get_parameters`, `get_custodians`, `get_balance_config`,
//!   `get_max_cleanup_operations` and `get_version`.
//!
//! Events are not bound here — the wallet emits its lifecycle events to
//! hardcoded external destinations (ids `1100`–`1116`), decodable through
//! [`crate::event`] with this ABI.
//!
//! # Reads go through storage, not get-methods
//!
//! Every read here decodes the account's data cell ([`Multisig::account_data`])
//! instead of executing a get-method. Running any get-method against this code
//! fails in `run_tvm` with
//!
//! ```text
//! code 404  TVM internal error: can not parse actions: 0
//! ```
//!
//! because `tvm_block`'s action-list parser does not recognise a tag emitted by
//! `sol 0.81.0` output (`tvm_block/src/out_actions.rs`; identical in tvm-sdk
//! 3.0.2 and 3.0.4, so bumping the SDK does not help). Verified on shellnet for
//! every read method of the previous `sol 0.81.0` build; this build comes from
//! the same compiler. Writes are unaffected — they go through `process_message`,
//! not `run_tvm`.
//!
//! Three consequences of reading storage:
//!
//! - `maxQueuedTransactions`, `maxCustodianCount`, `expirationTime` and the
//!   `getVersion` strings are compile-time constants of the contract and are
//!   not in the data cell. They are mirrored here as [`MAX_QUEUED_TRANSACTIONS`],
//!   [`MAX_CUSTODIAN_COUNT`], [`EXPIRATION_TIME`], [`VERSION`] and
//!   [`CONTRACT_NAME`], and must be re-checked whenever the assets are
//!   refreshed.
//! - Expiry is a property of a request id, which the contract compares against
//!   `block.timestamp`. Off-chain there is no block clock, so the list-shaped
//!   reads drop expired requests using the *client's* clock ([`expiration_bound`]),
//!   the way the contract's listing get-methods do. The by-id reads return a
//!   stored request even when it is expired — also matching the contract. The
//!   unfiltered queues are on [`AccountData`] if a caller wants them raw.
//! - Storage layout is build-specific: decoding a wallet deployed from any
//!   other build with this ABI is not valid.
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

use chrono::Utc;
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
use tvm_client::boc::ParamsOfGetBocHash;
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
pub const CODE_HASH: &str = "cfcaac10d43c8dc062298cb48df097be67cddec52b9cfd558309a7549f01c1f1";

/// `MAX_QUEUED_REQUESTS` — per-custodian cap on unconfirmed requests, applied
/// to each of the four queues separately. Contract constant, not stored.
pub const MAX_QUEUED_TRANSACTIONS: u8 = 5;

/// `MAX_CUSTODIAN_COUNT`. Contract constant, not stored.
pub const MAX_CUSTODIAN_COUNT: u8 = 32;

/// `EXPIRATION_TIME`, in seconds. Contract constant, not stored.
pub const EXPIRATION_TIME: u64 = 3601;

/// `ZERO_TIME` — epoch offset the contract folds into every request id.
/// Contract constant, not stored; used by [`expiration_bound`].
pub const ZERO_TIME: u64 = 1_000_000_000;

/// `MIN_CLEANUP_OPERATIONS` — smallest value `set_max_cleanup_operations`
/// accepts (the contract throws `124` below it). Contract constant, not stored.
pub const MIN_CLEANUP_OPERATIONS: u64 = 1;

/// `DEFAULT_CLEANUP_OPERATIONS` — the cleanup budget a wallet starts with,
/// written by the constructor and replaced by `set_max_cleanup_operations`.
/// Contract constant; the live value is stored, and is what
/// [`Multisig::get_max_cleanup_operations`] reads.
pub const DEFAULT_CLEANUP_OPERATIONS: u64 = 40;

/// First return value of `getVersion`. Contract constant, not stored.
pub const VERSION: &str = "2.4.0";

/// Second return value of `getVersion`. Contract constant, not stored.
pub const CONTRACT_NAME: &str = "UpdateCustodianMultisigWallet_v2";

/// Decoded persistent storage of the multisig wallet.
///
/// Mirrors the ABI's `fields` list one-to-one (`m_*` for state, `_*` for the
/// runtime preamble) via serde aliases, so a single `Account` decode hydrates
/// the struct directly. The maps are `BTreeMap` so iteration order is stable,
/// and they carry every stored request, expired ones included — the read
/// methods are what apply the contract's expiry filter.
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

    #[serde(alias = "m_requestsMaskCode")]
    pub requests_mask_code: String,

    /// Per-custodian request mask for queued balance-config updates.
    #[serde(alias = "m_requestsMaskConfig")]
    pub requests_mask_config: String,

    /// Queued transactions, keyed by transaction id (decimal string).
    #[serde(alias = "m_transactions", default)]
    pub transactions: BTreeMap<String, Transaction>,

    /// Queued custodian/confirmation updates, keyed by request id.
    #[serde(alias = "m_data", default)]
    pub data_updates: BTreeMap<String, DataUpdate>,

    /// Queued code updates, keyed by request id.
    #[serde(alias = "m_code", default)]
    pub code_updates: BTreeMap<String, CodeUpdate>,

    /// Queued balance-config updates, keyed by request id.
    #[serde(alias = "m_config", default)]
    pub config_updates: BTreeMap<String, ConfigUpdate>,

    /// Live gas self-management config.
    #[serde(alias = "m_balanceConfig")]
    pub balance_config: BalanceConfig,

    /// Custodians, keyed by the contract's `hash(pubkey, address)` index
    /// (`0x` + 64 hex), not by the custodian's own key.
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

/// Queued code update (`submitUpdateCode`), carrying the pending cells.
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

/// Queued code update without the cells, as the contract's listing get-method
/// reports it: a whole queue of [`CodeUpdate`]s would carry a copy of the
/// pending code each, so the code and its migration cell are identified by
/// hash. Use [`Multisig::get_update_code`] for one entry with its cells.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeUpdateInfo {
    pub id: String,
    #[serde(rename = "confirmationsMask")]
    pub confirmations_mask: String,
    #[serde(rename = "signsRequired")]
    pub signs_required: String,
    #[serde(rename = "signsReceived")]
    pub signs_received: String,
    /// Index of the custodian that queued the update — the creator's `index`,
    /// not the whole [`Custodian`].
    #[serde(rename = "creatorIndex")]
    pub creator_index: String,
    /// `0x` + 64 hex repr hash of `newcode`.
    #[serde(rename = "codeHash")]
    pub code_hash: String,
    /// `0x` + 64 hex repr hash of `cell`.
    #[serde(rename = "cellHash")]
    pub cell_hash: String,
}

/// Gas self-management config: below `min_balance` vmshell the wallet converts
/// SHELL up to `target_balance`. `min_balance == 0` disables auto top-up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceConfig {
    #[serde(rename = "minBalance")]
    pub min_balance: String,
    #[serde(rename = "targetBalance")]
    pub target_balance: String,
}

/// Queued balance-config update (`submitConfigUpdate`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdate {
    pub id: String,
    #[serde(rename = "confirmationsMask")]
    pub confirmations_mask: String,
    #[serde(rename = "signsRequired")]
    pub signs_required: String,
    #[serde(rename = "signsReceived")]
    pub signs_received: String,
    pub creator: Custodian,
    /// Config to apply once the update is confirmed.
    pub config: BalanceConfig,
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
    /// Destination contract address. The contract rejects the zero address
    /// (throws `125`).
    pub dest: String,
    /// Native vmshell value attached to the internal message. Ignored by the
    /// contract when `flag` carries `128` (send all remaining).
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

/// Parameters of `submitDataUpdate` — the new custodian set and its two
/// confirmation thresholds. Both thresholds must be `> 0` (the contract throws
/// `123`), and the two owner lists together must be non-empty and no longer
/// than [`MAX_CUSTODIAN_COUNT`] (`117`).
#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSubmitDataUpdate {
    /// Custodian pubkeys, as uint256 decimal/hex strings.
    pub owners_pubkey: Vec<String>,
    /// Custodian addresses.
    pub owners_address: Vec<String>,
    /// Confirmations required to execute a transaction.
    #[serde(rename = "reqConfirms")]
    pub req_confirms: u8,
    /// Confirmations required to apply a data, code or config update.
    #[serde(rename = "reqConfirmsData")]
    pub req_confirms_data: u8,
}

impl Default for ParamsOfSubmitDataUpdate {
    fn default() -> Self {
        Self {
            owners_pubkey: Default::default(),
            owners_address: Default::default(),
            req_confirms: 1,
            req_confirms_data: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfSubmitDataUpdate {
    #[serde(rename = "transId", deserialize_with = "deserialize_u64")]
    pub trans_id: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParamsOfConfirmDataUpdate {
    #[serde(rename = "dataUpdateId")]
    pub data_update_id: u64,
}

/// Parameters of `submitUpdateCode`.
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

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParamsOfConfirmUpdateCode {
    #[serde(rename = "codeUpdateId")]
    pub code_update_id: u64,
}

/// Parameters of `submitConfigUpdate`. `target_balance` must be `>=`
/// `min_balance` (the contract throws `126`); `min_balance == 0` disables
/// auto top-up.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ParamsOfSubmitConfigUpdate {
    #[serde(rename = "minBalance")]
    pub min_balance: u128,
    #[serde(rename = "targetBalance")]
    pub target_balance: u128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfSubmitConfigUpdate {
    #[serde(rename = "configUpdateId", deserialize_with = "deserialize_u64")]
    pub config_update_id: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParamsOfConfirmConfigUpdate {
    #[serde(rename = "configUpdateId")]
    pub config_update_id: u64,
}

/// Parameters of `setMaxCleanupOperations`. `value` must be `>=`
/// [`MIN_CLEANUP_OPERATIONS`] (the contract throws `124`).
#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfSetMaxCleanupOperations {
    pub value: u64,
}

impl Default for ParamsOfSetMaxCleanupOperations {
    fn default() -> Self {
        Self { value: DEFAULT_CLEANUP_OPERATIONS }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetTransaction {
    #[serde(rename = "transactionId")]
    pub transaction_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetUpdateData {
    #[serde(rename = "updateDataId")]
    pub update_data_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetUpdateCode {
    #[serde(rename = "codeUpdateId")]
    pub code_update_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetConfigUpdate {
    #[serde(rename = "configUpdateId")]
    pub config_update_id: u64,
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
pub struct ResultOfGetUpdateData {
    pub data: DataUpdate,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetUpdateDatas {
    pub data: Vec<DataUpdate>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetUpdateDataIds {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetUpdateCode {
    #[serde(rename = "codeUpdate")]
    pub code_update: CodeUpdate,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetUpdateCodes {
    #[serde(rename = "codeUpdates")]
    pub code_updates: Vec<CodeUpdateInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetUpdateCodeIds {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetBalanceConfig {
    pub config: BalanceConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetConfigUpdate {
    #[serde(rename = "configUpdate")]
    pub config_update: ConfigUpdate,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetConfigUpdates {
    #[serde(rename = "configUpdates")]
    pub config_updates: Vec<ConfigUpdate>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetConfigUpdateIds {
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
pub struct ResultOfGetMaxCleanupOperations {
    /// `uint256`, so it decodes as a `0x`-prefixed hex string — as it would
    /// from the get-method.
    #[serde(rename = "maxCleanupOperations")]
    pub max_cleanup_operations: String,
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
        self.call("submitTransaction", params, signer).await
    }

    /// # Send transaction
    ///
    /// Original contract method: `sendTransaction`
    ///
    /// Direct transfer with explicit flags, bypassing the confirmation
    /// queue. Only valid for single-custodian wallets (the contract throws
    /// `108` otherwise).
    pub async fn send_transaction(
        &self,
        params: ParamsOfSendTransaction,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        self.call("sendTransaction", params, signer).await
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
        self.call("confirmTransaction", params, signer).await
    }

    /// # Submit custodian-set update
    ///
    /// Original contract method: `submitDataUpdate`
    ///
    /// Queue a replacement of the custodian set and the two confirmation
    /// thresholds. Needs `reqConfirmsData` confirmations, and applying it
    /// clears every queue — pending transfers, data, code and config updates
    /// are discarded, since they were issued against the old custodian
    /// indices. The balance config is an operator setting and survives.
    pub async fn submit_data_update(
        &self,
        params: ParamsOfSubmitDataUpdate,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        self.call("submitDataUpdate", params, signer).await
    }

    /// # Confirm custodian-set update
    ///
    /// Original contract method: `confirmDataUpdate`
    pub async fn confirm_data_update(
        &self,
        params: ParamsOfConfirmDataUpdate,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        self.call("confirmDataUpdate", params, signer).await
    }

    /// # Submit code update
    ///
    /// Original contract method: `submitUpdateCode`
    ///
    /// Queue a code upgrade. Confirmed with `confirm_update_code`; applied
    /// once `reqConfirmsData` custodians have signed.
    pub async fn submit_update_code(
        &self,
        params: ParamsOfSubmitUpdateCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        self.call("submitUpdateCode", params, signer).await
    }

    /// # Confirm code update
    ///
    /// Original contract method: `confirmUpdateCode`
    pub async fn confirm_update_code(
        &self,
        params: ParamsOfConfirmUpdateCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        self.call("confirmUpdateCode", params, signer).await
    }

    /// # Submit balance-config update
    ///
    /// Original contract method: `submitConfigUpdate`
    ///
    /// Queue a change of the gas self-management thresholds. Needs
    /// `reqConfirmsData` confirmations; applied config is readable through
    /// [`Multisig::get_balance_config`].
    pub async fn submit_config_update(
        &self,
        params: ParamsOfSubmitConfigUpdate,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        self.call("submitConfigUpdate", params, signer).await
    }

    /// # Confirm balance-config update
    ///
    /// Original contract method: `confirmConfigUpdate`
    pub async fn confirm_config_update(
        &self,
        params: ParamsOfConfirmConfigUpdate,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        self.call("confirmConfigUpdate", params, signer).await
    }

    /// # Set cleanup budget
    ///
    /// Original contract method: `setMaxCleanupOperations`
    ///
    /// How many expired requests the wallet removes per cleanup pass. Any
    /// custodian can set it, without confirmations, and the value survives
    /// custodian changes.
    pub async fn set_max_cleanup_operations(
        &self,
        params: ParamsOfSetMaxCleanupOperations,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        self.call("setMaxCleanupOperations", params, signer).await
    }

    /// Fetches the account and decodes its persistent storage.
    ///
    /// This is the single network round-trip behind every read method; call it
    /// directly when you need more than one of them and want one fetch, or
    /// when you want the queues unfiltered.
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

    /// # Get cleanup budget
    ///
    /// Original contract method: `getMaxCleanupOperations`
    pub async fn get_max_cleanup_operations(&self) -> KitResult<ResultOfGetMaxCleanupOperations> {
        let data = self.account_data().await?;
        Ok(ResultOfGetMaxCleanupOperations { max_cleanup_operations: data.max_cleanup_operations })
    }

    /// # Get custodians
    ///
    /// Original contract method: `getCustodians`
    ///
    /// Ordered by the stored `hash(pubkey, address)` key, matching the on-chain
    /// dictionary iteration order (keys are zero-padded 64-hex, so
    /// lexicographic == numeric).
    pub async fn get_custodians(&self) -> KitResult<ResultOfGetCustodians> {
        let data = self.account_data().await?;
        Ok(ResultOfGetCustodians { custodians: data.custodians.into_values().collect() })
    }

    /// # Get wallet balance config
    ///
    /// Original contract method: `getBalanceConfig`
    pub async fn get_balance_config(&self) -> KitResult<ResultOfGetBalanceConfig> {
        let data = self.account_data().await?;
        Ok(ResultOfGetBalanceConfig { config: data.balance_config })
    }

    /// # Get all queued transactions
    ///
    /// Original contract method: `getTransactions`
    ///
    /// Expired requests are dropped (client clock — see the module docs).
    pub async fn get_transactions(&self) -> KitResult<ResultOfGetTransactions> {
        let data = self.account_data().await?;
        Ok(ResultOfGetTransactions { transactions: live_values(data.transactions) })
    }

    /// # Get one queued transaction
    ///
    /// Original contract method: `getTransaction`
    ///
    /// Errors when the id is not queued — the contract throws `102` in the
    /// same case. Returns the request even when it is expired.
    pub async fn get_transaction(
        &self,
        params: ParamsOfGetTransaction,
    ) -> KitResult<ResultOfGetTransaction> {
        let mut data = self.account_data().await?;
        let trans = data
            .transactions
            .remove(&params.transaction_id.to_string())
            .ok_or_else(|| self.not_queued("Transaction", params.transaction_id))?;
        Ok(ResultOfGetTransaction { trans })
    }

    /// # Get queued transaction ids
    ///
    /// Original contract method: `getTransactionIds`
    ///
    /// Expired requests are dropped, like `get_transactions`.
    pub async fn get_transaction_ids(&self) -> KitResult<ResultOfGetTransactionIds> {
        let data = self.account_data().await?;
        Ok(ResultOfGetTransactionIds { ids: live_ids(data.transactions) })
    }

    /// # Get all queued custodian-set updates
    ///
    /// Original contract method: `getUpdateDatas`
    ///
    /// Expired requests are dropped.
    pub async fn get_update_datas(&self) -> KitResult<ResultOfGetUpdateDatas> {
        let data = self.account_data().await?;
        Ok(ResultOfGetUpdateDatas { data: live_values(data.data_updates) })
    }

    /// # Get one queued custodian-set update
    ///
    /// Original contract method: `getUpdateData`
    ///
    /// Errors when the id is not queued (the contract throws `102`). Returns
    /// the request even when it is expired.
    pub async fn get_update_data(
        &self,
        params: ParamsOfGetUpdateData,
    ) -> KitResult<ResultOfGetUpdateData> {
        let mut data = self.account_data().await?;
        let update = data
            .data_updates
            .remove(&params.update_data_id.to_string())
            .ok_or_else(|| self.not_queued("Data update", params.update_data_id))?;
        Ok(ResultOfGetUpdateData { data: update })
    }

    /// # Get queued custodian-set update ids
    ///
    /// Original contract method: `getUpdateDataIds`
    pub async fn get_update_data_ids(&self) -> KitResult<ResultOfGetUpdateDataIds> {
        let data = self.account_data().await?;
        Ok(ResultOfGetUpdateDataIds { ids: live_ids(data.data_updates) })
    }

    /// # Get all queued code updates
    ///
    /// Original contract method: `getUpdateCodes`
    ///
    /// Identifies each pending code by hash, as the contract does; the hashes
    /// are computed here from the stored cells. Expired requests are dropped.
    pub async fn get_update_codes(&self) -> KitResult<ResultOfGetUpdateCodes> {
        let data = self.account_data().await?;
        let code_updates = live_values(data.code_updates)
            .into_iter()
            .map(|update| {
                Ok(CodeUpdateInfo {
                    id: update.id,
                    confirmations_mask: update.confirmations_mask,
                    signs_required: update.signs_required,
                    signs_received: update.signs_received,
                    creator_index: update.creator.index,
                    code_hash: self.cell_hash(&update.newcode)?,
                    cell_hash: self.cell_hash(&update.cell)?,
                })
            })
            .collect::<KitResult<Vec<_>>>()?;
        Ok(ResultOfGetUpdateCodes { code_updates })
    }

    /// # Get one queued code update
    ///
    /// Original contract method: `getUpdateCode`
    ///
    /// Carries the pending cells, so a custodian can inspect the code before
    /// confirming it. Errors when the id is not queued (the contract throws
    /// `102`). Returns the request even when it is expired.
    pub async fn get_update_code(
        &self,
        params: ParamsOfGetUpdateCode,
    ) -> KitResult<ResultOfGetUpdateCode> {
        let mut data = self.account_data().await?;
        let code_update = data
            .code_updates
            .remove(&params.code_update_id.to_string())
            .ok_or_else(|| self.not_queued("Code update", params.code_update_id))?;
        Ok(ResultOfGetUpdateCode { code_update })
    }

    /// # Get queued code update ids
    ///
    /// Original contract method: `getUpdateCodeIds`
    pub async fn get_update_code_ids(&self) -> KitResult<ResultOfGetUpdateCodeIds> {
        let data = self.account_data().await?;
        Ok(ResultOfGetUpdateCodeIds { ids: live_ids(data.code_updates) })
    }

    /// # Get all queued balance-config updates
    ///
    /// Original contract method: `getConfigUpdates`
    ///
    /// Expired requests are dropped.
    pub async fn get_config_updates(&self) -> KitResult<ResultOfGetConfigUpdates> {
        let data = self.account_data().await?;
        Ok(ResultOfGetConfigUpdates { config_updates: live_values(data.config_updates) })
    }

    /// # Get one queued balance-config update
    ///
    /// Original contract method: `getConfigUpdate`
    ///
    /// Errors when the id is not queued (the contract throws `102`). Returns
    /// the request even when it is expired.
    pub async fn get_config_update(
        &self,
        params: ParamsOfGetConfigUpdate,
    ) -> KitResult<ResultOfGetConfigUpdate> {
        let mut data = self.account_data().await?;
        let config_update = data
            .config_updates
            .remove(&params.config_update_id.to_string())
            .ok_or_else(|| self.not_queued("Config update", params.config_update_id))?;
        Ok(ResultOfGetConfigUpdate { config_update })
    }

    /// # Get queued balance-config update ids
    ///
    /// Original contract method: `getConfigUpdateIds`
    pub async fn get_config_update_ids(&self) -> KitResult<ResultOfGetConfigUpdateIds> {
        let data = self.account_data().await?;
        Ok(ResultOfGetConfigUpdateIds { ids: live_ids(data.config_updates) })
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

    /// Encodes `function_name(params)` and sends it as an external message.
    async fn call(
        &self,
        function_name: &str,
        params: impl Serialize,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: function_name.to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Repr hash of a stored cell, in the `0x` + 64 hex form the contract's
    /// `uint256` outputs decode to.
    fn cell_hash(&self, boc: &str) -> KitResult<String> {
        let hash = tvm_client::boc::get_boc_hash(
            self.context().clone(),
            ParamsOfGetBocHash { boc: boc.to_string() },
        )
        .map_err(|e| {
            KitError::new(Self::MODULE, KitErrorCode::Decode, "Get cell hash").with_tvm_error(e)
        })?
        .hash;
        Ok(format!("0x{hash}"))
    }

    fn not_queued(&self, kind: &str, id: u64) -> KitError {
        KitError::new(
            Self::MODULE,
            KitErrorCode::EmptyResult,
            format!("{kind} `{id}` is not queued on `{}`", self.address()),
        )
    }
}

/// Mirrors the contract's `isConfirmed`: whether the custodian at `index` has
/// already signed the request carrying `mask`.
pub fn is_confirmed(mask: u32, index: u8) -> bool {
    index < u32::BITS as u8 && mask & (1 << index) != 0
}

/// Lower bound on live request ids, mirroring the contract's
/// `_getExpirationBound()`: a request is expired once its id is `<=` the bound.
///
/// Request ids embed their submission time (`(block.timestamp - ZERO_TIME) << 32`),
/// which is what makes this comparison work. `now` is unix seconds.
pub fn expiration_bound(now: u64) -> u64 {
    now.saturating_sub(EXPIRATION_TIME).saturating_sub(ZERO_TIME).saturating_mul(1 << 32)
}

/// Live entries of a decoded queue, ordered by id.
fn live_entries<T>(queue: BTreeMap<String, T>) -> Vec<(String, T)> {
    let bound = expiration_bound(now_secs());
    let mut entries: Vec<(String, T)> =
        queue.into_iter().filter(|(id, _)| !is_expired(id, bound)).collect();
    entries.sort_by(|(a, _), (b, _)| numeric_id_cmp(a, b));
    entries
}

fn live_values<T>(queue: BTreeMap<String, T>) -> Vec<T> {
    live_entries(queue).into_iter().map(|(_, value)| value).collect()
}

fn live_ids<T>(queue: BTreeMap<String, T>) -> Vec<String> {
    live_entries(queue).into_iter().map(|(id, _)| id).collect()
}

/// Ids are decimal `uint64` map keys; one that does not parse is kept rather
/// than silently dropped.
fn is_expired(id: &str, bound: u64) -> bool {
    id.parse::<u64>().is_ok_and(|id| id <= bound)
}

/// The contract compares request ids against `block.timestamp`, which is not
/// available off-chain — the client's clock stands in for it.
fn now_secs() -> u64 {
    Utc::now().timestamp().max(0) as u64
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

    use super::expiration_bound;
    use super::is_confirmed;
    use super::is_expired;
    use super::live_ids;
    use super::now_secs;
    use super::numeric_id_cmp;
    use super::AccountData;
    use super::ParamsOfConfirmConfigUpdate;
    use super::ParamsOfConfirmDataUpdate;
    use super::ParamsOfConfirmTransaction;
    use super::ParamsOfConfirmUpdateCode;
    use super::ParamsOfGetConfigUpdate;
    use super::ParamsOfGetTransaction;
    use super::ParamsOfGetUpdateCode;
    use super::ParamsOfGetUpdateData;
    use super::ParamsOfSendTransaction;
    use super::ParamsOfSetMaxCleanupOperations;
    use super::ParamsOfSubmitConfigUpdate;
    use super::ParamsOfSubmitDataUpdate;
    use super::ParamsOfSubmitTransaction;
    use super::ParamsOfSubmitUpdateCode;
    use super::ABI;
    use super::CODE_HASH;
    use super::EXPIRATION_TIME;
    use super::ZERO_TIME;
    use crate::tests::create_context;

    const TVC: &[u8] = include_bytes!("../../abi/multisig/Multisig.tvc");

    /// sha256 of the vendored TVC.
    const TVC_SHA256: &str = "b0d72acbbdc6af309823e74b96b0b3ffb0f871a5b98316b6e89affdfb56c5c9d";

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

    fn assert_params_match_abi(func: &str, params: impl serde::Serialize) {
        let value = serde_json::to_value(params).expect("serialize params");
        assert_params_cover_abi(func, &value);
    }

    #[test]
    fn write_params_match_abi() {
        assert_params_match_abi("submitTransaction", ParamsOfSubmitTransaction::default());
        assert_params_match_abi("sendTransaction", ParamsOfSendTransaction::default());
        assert_params_match_abi(
            "confirmTransaction",
            ParamsOfConfirmTransaction { transaction_id: 1 },
        );
        assert_params_match_abi("submitDataUpdate", ParamsOfSubmitDataUpdate::default());
        assert_params_match_abi("confirmDataUpdate", ParamsOfConfirmDataUpdate::default());
        assert_params_match_abi("submitUpdateCode", ParamsOfSubmitUpdateCode::default());
        assert_params_match_abi("confirmUpdateCode", ParamsOfConfirmUpdateCode::default());
        assert_params_match_abi("submitConfigUpdate", ParamsOfSubmitConfigUpdate::default());
        assert_params_match_abi("confirmConfigUpdate", ParamsOfConfirmConfigUpdate::default());
        assert_params_match_abi(
            "setMaxCleanupOperations",
            ParamsOfSetMaxCleanupOperations::default(),
        );
    }

    /// The by-id reads take the same inputs as the get-methods they mirror,
    /// even though they answer from storage — a renamed id would make a
    /// caller's code silently non-portable.
    #[test]
    fn read_params_match_abi() {
        assert_params_match_abi("getTransaction", ParamsOfGetTransaction { transaction_id: 1 });
        assert_params_match_abi("getUpdateData", ParamsOfGetUpdateData { update_data_id: 1 });
        assert_params_match_abi("getUpdateCode", ParamsOfGetUpdateCode { code_update_id: 1 });
        assert_params_match_abi("getConfigUpdate", ParamsOfGetConfigUpdate { config_update_id: 1 });
    }

    /// The bundled ABI must be the v2.4 build: the config-update queue and the
    /// stored balance config are what tell it apart from the earlier v2 builds,
    /// which share every other function.
    #[test]
    fn bundled_abi_is_update_custodian_v2_4() {
        let abi = abi_json();

        assert_eq!(abi["version"].as_str(), Some("2.4"), "unexpected ABI version");

        let functions: Vec<&str> = abi["functions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["name"].as_str().unwrap())
            .collect();
        for name in [
            "submitUpdateCode",
            "confirmUpdateCode",
            "submitConfigUpdate",
            "confirmConfigUpdate",
            "getBalanceConfig",
            "getUpdateCode",
            "getUpdateCodes",
            "getConfigUpdates",
            "getMaxCleanupOperations",
        ] {
            assert!(functions.contains(&name), "ABI is missing v2.4 function `{name}`");
        }

        let fields: Vec<&str> =
            abi["fields"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
        for name in
            ["m_requestsMaskCode", "m_code", "m_requestsMaskConfig", "m_config", "m_balanceConfig"]
        {
            assert!(fields.contains(&name), "ABI is missing v2.4 storage field `{name}`");
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
        assert!(account_data.data_updates.is_empty());
        assert!(account_data.code_updates.is_empty());
        assert!(account_data.config_updates.is_empty());

        // The balance config is a plain struct field, so this is also what
        // proves the nested decode into `BalanceConfig` holds.
        assert_eq!(account_data.balance_config.min_balance, "0");
        assert_eq!(account_data.balance_config.target_balance, "0");

        // `uint256` fields decode as `0x` + 64 hex, unlike the narrower ints.
        assert_eq!(account_data.max_cleanup_operations, format!("0x{:064x}", 0));
    }

    #[test]
    fn transaction_ids_sort_numerically() {
        let mut ids = ["10".to_string(), "9".to_string(), "100".to_string(), "11".to_string()];
        ids.sort_by(|a, b| numeric_id_cmp(a, b));
        assert_eq!(ids, ["9", "10", "11", "100"]);
    }

    /// The bound must match `_getExpirationBound()`, and ids are compared
    /// against it the way the listing get-methods do (`id > bound` survives).
    #[test]
    fn expired_requests_are_dropped_from_listings() {
        let now = 1_800_000_000;
        let bound = expiration_bound(now);
        assert_eq!(bound, (now - EXPIRATION_TIME - ZERO_TIME) << 32);

        // Ids embed their submission second in the high 32 bits.
        let id_at = |submitted_at: u64| ((submitted_at - ZERO_TIME) << 32) | 7;

        let fresh = id_at(now - 60);
        let expired = id_at(now - EXPIRATION_TIME - 60);
        assert!(!is_expired(&fresh.to_string(), bound));
        assert!(is_expired(&expired.to_string(), bound));

        // An unparsable key is kept rather than dropped.
        assert!(!is_expired("0x2a", bound));
    }

    #[test]
    fn live_ids_are_ordered_and_filtered() {
        let now = now_secs();
        let id_at = |submitted_at: u64| (((submitted_at - ZERO_TIME) << 32) | 1).to_string();

        let queue = std::collections::BTreeMap::from([
            (id_at(now - 30), ()),
            (id_at(now - 120), ()),
            (id_at(now - EXPIRATION_TIME - 600), ()),
        ]);

        let ids = live_ids(queue);
        assert_eq!(ids, [id_at(now - 120), id_at(now - 30)]);
    }

    #[test]
    fn confirmation_mask_is_read_per_custodian() {
        let mask = (1 << 0) | (1 << 5);
        assert!(is_confirmed(mask, 0));
        assert!(is_confirmed(mask, 5));
        assert!(!is_confirmed(mask, 1));
        // Out-of-range index cannot shift past the mask width.
        assert!(!is_confirmed(u32::MAX, 32));
    }
}
