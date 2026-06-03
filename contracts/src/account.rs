use std::collections::BTreeMap;
use std::sync::Arc;

use num_bigint::BigInt;
use serde::Deserialize;
use serde::Serialize;
use shared::utils::sleep_ms;
use tvm_block::Deserializable;
use tvm_client::account::ParamsOfGetAccount;
use tvm_client::boc::ParamsOfParse;
use tvm_client::ClientContext;

use crate::deserialize::deserialize_account_balance;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::KitResult;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[repr(u8)]
#[serde(from = "u8")]
pub enum AccountStatus {
    Uninit = 0,
    Active = 1,
    Frozen = 2,
    NonExist = 3,
}

impl From<u8> for AccountStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => AccountStatus::Uninit,
            1 => AccountStatus::Active,
            2 => AccountStatus::Frozen,
            3 => AccountStatus::NonExist,
            _ => AccountStatus::NonExist,
        }
    }
}

/// Identity parameters for constructing a contract wrapper.
///
/// Future-proof: new identity fields can be added here without changing the
/// `new(...)` signature of every contract again.
///
/// `dapp_id` is the bare 64-char hex dApp ID (no `0x`, no workchain) and is
/// **mandatory** — every contract belongs to a dApp. It is consumed by the SDK
/// on `>= 1.0.0` servers and simply ignored on legacy (`< 1.0.0`) ones.
#[derive(Debug, Clone)]
pub struct ParamsOfNewContract {
    /// Raw account address, e.g. `"0:<64hex>"`.
    pub address: String,
    /// Bare 64-hex dApp ID (no `0x`, no workchain). Required.
    pub dapp_id: String,
}

impl ParamsOfNewContract {
    pub fn new(address: impl Into<String>, dapp_id: impl Into<String>) -> Self {
        Self { address: address.into(), dapp_id: dapp_id.into() }
    }
}

/// Extracts the bare account-id hex from an address, dropping the workchain
/// prefix (`"0:<hex>"` -> `"<hex>"`). Returns the input unchanged if it has no
/// `:` separator.
pub(crate) fn account_id_from_address(address: &str) -> &str {
    address.rsplit_once(':').map(|(_, id)| id).unwrap_or(address)
}

#[derive(Debug, Clone)]
pub struct Account {
    pub context: Arc<ClientContext>,
    pub address: String,
    /// Bare 64-hex dApp ID. Required; ignored by the SDK on legacy (`< 1.0.0`) servers.
    pub dapp_id: String,
    pub boc: Option<String>,
    pub data: Option<String>,
    pub balance: Option<BigInt>,
    pub acc_type: AccountStatus,
    pub code_hash: Option<String>,
    pub ecc: BTreeMap<u32, BigInt>,
}

#[derive(Debug, Deserialize)]
struct AccountData {
    boc: Option<String>,
    data: Option<String>,
    #[serde(deserialize_with = "deserialize_account_balance")]
    balance: Option<BigInt>,
    acc_type: AccountStatus,
    code_hash: Option<String>,
}

#[derive(Debug)]
pub struct ParamsOfWaitAccount {
    pub status: AccountStatus,
    pub attempts: Option<u8>,
    pub attempts_timeout: Option<u64>,
}

impl Default for ParamsOfWaitAccount {
    fn default() -> Self {
        Self { status: AccountStatus::Active, attempts: Some(10), attempts_timeout: Some(1000) }
    }
}

impl Account {
    pub fn new(
        context: Arc<ClientContext>,
        address: impl AsRef<str>,
        dapp_id: impl Into<String>,
    ) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            dapp_id: dapp_id.into(),
            boc: None,
            data: None,
            balance: None,
            acc_type: AccountStatus::NonExist,
            code_hash: None,
            ecc: BTreeMap::new(),
        }
    }

    fn reset(&mut self) {
        self.boc = None;
        self.data = None;
        self.acc_type = AccountStatus::NonExist;
        self.balance = None;
        self.code_hash = None;
        self.ecc = BTreeMap::new();
    }

    pub fn is_deployed(&self) -> bool {
        self.acc_type == AccountStatus::Active
    }

    pub async fn fetch(&mut self) -> KitResult<()> {
        // Fetch account boc
        let get_account_result = tvm_client::account::get_account(
            self.context.clone(),
            ParamsOfGetAccount {
                account_id: account_id_from_address(&self.address).to_string(),
                // Ignored by the SDK on legacy (`< 1.0.0`) servers.
                dapp_id: self.dapp_id.clone(),
            },
        )
        .await;

        let boc = match get_account_result {
            Ok(result) => result.boc,
            Err(e) => match e.code() {
                622 => {
                    tracing::warn!(target: "ackinacki_kit", "Get account `{}` ({e})", self.address);
                    self.reset();
                    return Ok(());
                }
                _ => {
                    return Err(KitError::new(
                        KitModule::Account,
                        KitErrorCode::GetAccount,
                        format!("Get account `{}` ({e})", self.address),
                    )
                    .with_tvm_error(e));
                }
            },
        };

        // Construct account from boc to get ecc balance
        let tvm_account = tvm_block::Account::construct_from_base64(&boc).map_err(|e| {
            KitError::new(
                KitModule::Account,
                KitErrorCode::ConstructAccount,
                format!("Construct account `{}` from boc ({e})", self.address),
            )
        })?;

        // Parse account boc
        let parsed = tvm_client::boc::parse_account(
            self.context.clone(),
            ParamsOfParse { boc: boc.clone() },
        )
        .map_err(|e| {
            KitError::new(
                KitModule::Account,
                KitErrorCode::ParseAccount,
                format!("Parse account `{}` ({e})", self.address),
            )
        })?
        .parsed;

        // Deserialize account value
        let deserialized = serde_json::from_value::<AccountData>(parsed).map_err(|e| {
            KitError::new(
                KitModule::Account,
                KitErrorCode::DeserializeAccountData,
                format!("Deserialize account `{}` ({e})", self.address),
            )
        })?;

        self.boc = deserialized.boc;
        self.data = deserialized.data;
        self.acc_type = deserialized.acc_type;
        self.balance = deserialized.balance;
        self.code_hash = deserialized.code_hash;
        self.ecc = match tvm_account.balance() {
            Some(balance) => {
                let mut map = BTreeMap::new();
                balance
                    .other
                    .iterate_with_keys::<u32, _>(|k, v| {
                        map.insert(k, v.value().clone());
                        Ok(true)
                    })
                    .map_err(|e| {
                        KitError::new(
                            KitModule::Account,
                            KitErrorCode::IterateCurrencies,
                            format!("Iterate account `{}` currency ({e})", self.address),
                        )
                    })?;
                map
            }
            None => BTreeMap::new(),
        };

        Ok(())
    }

    pub async fn wait(&mut self, params: ParamsOfWaitAccount) -> KitResult<()> {
        let mut attempts = 0;
        loop {
            if attempts == params.attempts.unwrap_or(20) {
                return Err(KitError::new(
                    KitModule::Account,
                    KitErrorCode::WaitAccount,
                    format!(
                        "Wait for account `{}` status `{:?}`. Max attempts reached.",
                        self.address, params.status,
                    ),
                ));
            }

            self.fetch().await?;
            if self.acc_type == params.status {
                return Ok(());
            }

            attempts += 1;

            let timeout = params.attempts_timeout.unwrap_or(1000);
            sleep_ms(timeout).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use crate::account::Account;
    use crate::account::AccountStatus;
    use crate::account::ParamsOfWaitAccount;
    use crate::tests::create_context;

    #[tokio::test]
    async fn test_fetch_account() {
        let context = create_context();
        let mut account = Account::new(
            context,
            "0:2222222222222222222222222222222222222222222222222222222222222222",
            crate::dapp::SystemDapp::System,
        );
        let fetch_result =
            account.fetch().await.inspect_err(|e| eprintln!("Fetch account ({e:?})"));
        assert!(fetch_result.is_ok());

        assert!(account.boc.is_some());
        assert!(account.data.is_some());
        assert!(account.balance.is_some() && account.balance.as_ref().unwrap() > &BigInt::ZERO);
        assert!(account.is_deployed());
    }

    #[tokio::test]
    async fn test_wait_account() {
        let context = create_context();

        // Wait for existing account
        let mut account = Account::new(
            context.clone(),
            "0:2222222222222222222222222222222222222222222222222222222222222222",
            crate::dapp::SystemDapp::System,
        );
        let wait_result = account
            .wait(ParamsOfWaitAccount { status: AccountStatus::Active, ..Default::default() })
            .await
            .inspect_err(|e| eprintln!("Wait account ({e:?})"));
        assert!(wait_result.is_ok());

        // Wait for non existing account
        let mut account = Account::new(
            context.clone(),
            "0:2222222222222222222222222222222222222222222222222222222222222220",
            crate::dapp::SystemDapp::System,
        );
        let wait_result = account
            .wait(ParamsOfWaitAccount {
                status: AccountStatus::Active,
                attempts: Some(3),
                attempts_timeout: Some(500),
            })
            .await
            .inspect_err(|e| eprintln!("Wait account ({e:?})"));
        assert!(wait_result.is_err());
    }

    #[tokio::test]
    async fn test_decode_account() {
        let context = create_context();

        // Wait for existing account
        let mut account = Account::new(
            context.clone(),
            "0:269840b497d21dc35c73ccfd31158eade4245ba01230196842acd5f8f3655011",
            crate::dapp::SystemDapp::System,
        );
        account.fetch().await.inspect_err(|e| eprintln!("Fetch account ({e:?})")).unwrap();
    }

    #[test]
    fn account_id_from_address_strips_workchain() {
        use crate::account::account_id_from_address;

        let hex = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        // Basechain / masterchain prefixes are dropped.
        assert_eq!(account_id_from_address(&format!("0:{hex}")), hex);
        assert_eq!(account_id_from_address(&format!("-1:{hex}")), hex);
        // Already bare → unchanged.
        assert_eq!(account_id_from_address(hex), hex);
        // Extended `<dapp>::<account>` → returns the account part (last segment).
        assert_eq!(account_id_from_address(&format!("{hex}::{hex}")), hex);
    }

    #[test]
    fn params_new_sets_dapp_id() {
        use crate::account::ParamsOfNewContract;

        let p = ParamsOfNewContract::new("0:ab", "deadbeef");
        assert_eq!(p.address, "0:ab");
        assert_eq!(p.dapp_id, "deadbeef");
    }

    #[test]
    fn params_new_accepts_system_dapp() {
        use crate::account::ParamsOfNewContract;
        use crate::dapp::SystemDapp;

        // `SystemDapp` plugs straight into `new` via `Into<String>`.
        let p = ParamsOfNewContract::new("0:ab", SystemDapp::AuthService);
        assert_eq!(p.dapp_id, SystemDapp::AuthService.dapp_id());
    }
}
