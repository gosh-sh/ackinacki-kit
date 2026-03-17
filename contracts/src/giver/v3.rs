use std::collections::HashMap;
use std::sync::Arc;

use num_bigint::BigInt;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use shared::utils::sleep_ms;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u32;
use crate::error::GiverModule;
use crate::error::KitError;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/giver/GiverV3.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for `GiverV3` funding contract.
pub struct GiverV3 {
    base: ContractBase,
}

impl ModuleAccessor for GiverV3 {
    const MODULE: KitModule = KitModule::Giver(GiverModule::V3);
}

impl HasContractBase for GiverV3 {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for GiverV3 {}

impl AsyncGuarded<Account> for GiverV3 {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for GiverV3 {
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
/// Parameters for `GiverV3.sendTransaction`.
pub struct ParamsOfSendTransaction {
    pub dest: String,
    pub value: u64,
    pub bounce: bool,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.sendCurrency`.
pub struct ParamsOfSendCurrency {
    pub dest: String,
    pub value: u64,
    pub ecc: HashMap<u32, u64>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.sendCurrencyWithFlag`.
pub struct ParamsOfSendCurrencyWithFlag {
    pub dest: String,
    pub value: u64,
    pub ecc: HashMap<u32, u64>,
    pub flag: u8,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.sendFreeToken`.
pub struct ParamsOfSendFreeToken {
    pub dest: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.getData`.
pub struct ParamsOfGetData {
    pub name: String,
    pub decimals: u128,
    #[serde(rename(serialize = "walletCode"))]
    pub wallet_code: String,
    #[serde(rename(serialize = "transactionCode"))]
    pub transaction_code: String,
    pub pubkey: String,
    #[serde(rename(serialize = "mintDisabled"))]
    pub mint_disabled: bool,
    #[serde(rename(serialize = "initialSupplyToOwner"))]
    pub initial_supply_to_owner: String,
    #[serde(rename(serialize = "initialSupply"))]
    pub initial_supply: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.getDataForPMP`.
pub struct ParamsOfGetDataForPmp {
    #[serde(rename(serialize = "PMPCode"))]
    pub pmp_code: String,
    #[serde(rename(serialize = "PMPWalletCode"))]
    pub pmp_wallet_code: String,
    #[serde(rename(serialize = "NullifierCode"))]
    pub nullifier_code: String,
    #[serde(rename(serialize = "OracleCode"))]
    pub oracle_code: String,
    #[serde(rename(serialize = "OracleEventListCode"))]
    pub oracle_event_list_code: String,
    #[serde(rename(serialize = "OrderBookCode"))]
    pub order_book_code: String,
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.getDataForOracle`.
pub struct ParamsOfGetDataForOracle {
    #[serde(rename(serialize = "PMPCode"))]
    pub pmp_code: String,
    #[serde(rename(serialize = "PMPWalletCode"))]
    pub pmp_wallet_code: String,
    #[serde(rename(serialize = "OracleCode"))]
    pub oracle_code: String,
    #[serde(rename(serialize = "OracleEventListCode"))]
    pub oracle_event_list_code: String,
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.getDataForVault`.
pub struct ParamsOfGetDataForVault {
    #[serde(rename(serialize = "PMPWalletCode"))]
    pub pmp_wallet_code: String,
    pub pubkey: String,
    pub root: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.getDataForAuthService`.
pub struct ParamsOfGetDataForAuthService {
    #[serde(rename(serialize = "profileCode"))]
    pub profile_code: String,
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `GiverV3.upgrade`.
pub struct ParamsOfUpgrade {
    pub newcode: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Single message metadata from `GiverV3.getMessages`.
pub struct GiverMessage {
    pub hash: String,
    #[serde(rename = "expireAt", deserialize_with = "deserialize_u32")]
    pub expire_at: u32,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `GiverV3.getMessages`.
pub struct ResultOfGetMessages {
    pub messages: Vec<GiverMessage>,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `getData*` getters.
pub struct ResultOfGetDataCell {
    #[serde(rename = "value0")]
    pub value: String,
}

impl GiverV3 {
    /// Default shellnet giver address.
    pub const DEFAULT_ADDRESS: &'static str =
        "0:1111111111111111111111111111111111111111111111111111111111111111";

    /// Creates wrapper for a deployed giver.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// Creates wrapper for default shellnet giver.
    pub fn new_default(context: Arc<ClientContext>) -> Self {
        Self::new(context, Self::DEFAULT_ADDRESS)
    }

    /// Original contract method: `sendTransaction`.
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

    /// Original contract method: `sendCurrency`.
    pub async fn send_currency(
        &self,
        params: ParamsOfSendCurrency,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "sendCurrency".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `sendCurrencyWithFlag`.
    pub async fn send_currency_with_flag(
        &self,
        params: ParamsOfSendCurrencyWithFlag,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "sendCurrencyWithFlag".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `sendFreeToken`.
    pub async fn send_free_token(
        &self,
        params: ParamsOfSendFreeToken,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "sendFreeToken".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `getMessages`.
    pub async fn get_messages(&self) -> KitResult<ResultOfGetMessages> {
        self.call_get_method::<ResultOfGetMessages>("getMessages").await
    }

    /// Original contract method: `getData`.
    pub async fn get_data(&self, params: ParamsOfGetData) -> KitResult<ResultOfGetDataCell> {
        self.call_get_method_with::<ResultOfGetDataCell, ParamsOfGetData>("getData", params).await
    }

    /// Original contract method: `getDataForPMP`.
    pub async fn get_data_for_pmp(
        &self,
        params: ParamsOfGetDataForPmp,
    ) -> KitResult<ResultOfGetDataCell> {
        self.call_get_method_with::<ResultOfGetDataCell, ParamsOfGetDataForPmp>(
            "getDataForPMP",
            params,
        )
        .await
    }

    /// Original contract method: `getDataForOracle`.
    pub async fn get_data_for_oracle(
        &self,
        params: ParamsOfGetDataForOracle,
    ) -> KitResult<ResultOfGetDataCell> {
        self.call_get_method_with::<ResultOfGetDataCell, ParamsOfGetDataForOracle>(
            "getDataForOracle",
            params,
        )
        .await
    }

    /// Original contract method: `getDataForVault`.
    pub async fn get_data_for_vault(
        &self,
        params: ParamsOfGetDataForVault,
    ) -> KitResult<ResultOfGetDataCell> {
        self.call_get_method_with::<ResultOfGetDataCell, ParamsOfGetDataForVault>(
            "getDataForVault",
            params,
        )
        .await
    }

    /// Original contract method: `getDataForAuthService`.
    pub async fn get_data_for_auth_service(
        &self,
        params: ParamsOfGetDataForAuthService,
    ) -> KitResult<ResultOfGetDataCell> {
        self.call_get_method_with::<ResultOfGetDataCell, ParamsOfGetDataForAuthService>(
            "getDataForAuthService",
            params,
        )
        .await
    }

    /// Original contract method: `upgrade`.
    pub async fn upgrade(
        &self,
        params: ParamsOfUpgrade,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "upgrade".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}

fn is_duplicate_message_error(err: &KitError) -> bool {
    let Some(tvm_err) = err.tvm_error.as_ref() else {
        return false;
    };

    tvm_err.code == 621
        && tvm_err
            .data
            .pointer("/node_error/extensions/code")
            .and_then(|v| v.as_str())
            .map(|v| v.eq_ignore_ascii_case("DUPLICATE_MESSAGE"))
            .unwrap_or(false)
}

/// Sends funds from default giver and ignores duplicate-message race on retries.
pub async fn send_currency_with_flag_from_default_giver(
    context: Arc<ClientContext>,
    dest: &str,
    native_value: u64,
    ecc: HashMap<u32, u64>,
    flag: u8,
) {
    let giver = GiverV3::new_default(context);
    let params =
        ParamsOfSendCurrencyWithFlag { dest: dest.to_string(), value: native_value, ecc, flag };

    match giver.send_currency_with_flag(params, Signer::None).await {
        Ok(_) => {}
        Err(err) if is_duplicate_message_error(&err) => {
            eprintln!(
                "send_currency_with_flag_from_default_giver: duplicate message, continue: {err:?}"
            );
        }
        Err(err) => {
            panic!("send GiverV3.sendCurrencyWithFlag: {err:?}");
        }
    }
}

/// Tops up native balance from giver when account balance is below threshold.
pub async fn top_up_native_with_giver_if_below<T>(
    context: Arc<ClientContext>,
    contract: &T,
    min_native_balance: u64,
    top_up_native_value: u64,
    label: &str,
) where
    T: AccountAccessor + AddressAccessor,
{
    async fn fetch_account_with_retry<T: AccountAccessor + AddressAccessor>(
        contract: &T,
        label: &str,
        phase: &str,
    ) {
        let max_attempts = 4;
        for attempt in 1..=max_attempts {
            match contract.fetch_account().await {
                Ok(()) => return,
                Err(err) => {
                    let msg = err
                        .tvm_error
                        .as_ref()
                        .map(|e| e.message.to_ascii_lowercase())
                        .unwrap_or_default();
                    let transient = msg.contains("connection reset by peer")
                        || msg.contains("client error (sendrequest)")
                        || msg.contains("all attempts failed");

                    if transient && attempt < max_attempts {
                        eprintln!(
                            "{label}: fetch_account {phase} transient network error on attempt {attempt}/{max_attempts}: {err:?}"
                        );
                        sleep_ms(700).await;
                        continue;
                    }

                    panic!("{label}: fetch_account {phase} failed: {err:?}");
                }
            }
        }
    }

    fetch_account_with_retry(contract, label, "before top-up check").await;
    let current_balance = {
        let guard = contract.account().lock().await;
        guard.balance.clone().unwrap_or_else(|| BigInt::from(0_u8))
    };

    let min_native = BigInt::from(min_native_balance);
    if current_balance >= min_native {
        return;
    }

    eprintln!(
        "{label} native balance {:?} is below {:?}; topping up via giver",
        current_balance, min_native
    );

    send_currency_with_flag_from_default_giver(
        context,
        contract.address(),
        top_up_native_value,
        HashMap::new(),
        1,
    )
    .await;

    sleep_ms(3_000).await;
    fetch_account_with_retry(contract, label, "after top-up").await;

    let guard = contract.account().lock().await;
    eprintln!("{label} after top-up: balance={:?}, ecc={:?}", guard.balance, guard.ecc);
}
