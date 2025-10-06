use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use num_bigint::BigInt;
use serde::Deserialize;
use serde::Serialize;
use tvm_block::Deserializable;
use tvm_client::account::ParamsOfGetAccount;
use tvm_client::boc::ParamsOfParse;
use tvm_client::ClientContext;

use crate::deserialize::deserialize_account_balance;

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

#[derive(Debug, Clone)]
pub struct Account {
    pub context: Arc<ClientContext>,
    pub address: String,
    pub boc: Option<String>,
    pub data: Option<String>,
    pub balance: Option<BigInt>,
    pub acc_type: AccountStatus,
    pub ecc: BTreeMap<u32, BigInt>,
}

#[derive(Debug, Deserialize)]
struct AccountData {
    boc: Option<String>,
    data: Option<String>,
    #[serde(deserialize_with = "deserialize_account_balance")]
    balance: Option<BigInt>,
    acc_type: AccountStatus,
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
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            boc: None,
            data: None,
            balance: None,
            acc_type: AccountStatus::NonExist,
            ecc: BTreeMap::new(),
        }
    }

    fn reset(&mut self) {
        self.boc = None;
        self.data = None;
        self.acc_type = AccountStatus::NonExist;
        self.balance = None;
        self.ecc = BTreeMap::new();
    }

    pub fn is_deployed(&self) -> bool {
        self.acc_type == AccountStatus::Active
    }

    pub async fn fetch(&mut self) -> anyhow::Result<()> {
        // Fetch account boc
        let get_account_result = tvm_client::account::get_account(
            self.context.clone(),
            ParamsOfGetAccount { address: self.address.clone() },
        )
        .await;

        let boc = match get_account_result {
            Ok(result) => result.boc,
            Err(e) => match e.code {
                622 => {
                    tracing::warn!(target: "ackinacki_kit", "Get account `{}` ({e})", self.address);
                    self.reset();
                    return Ok(());
                }
                _ => anyhow::bail!("Get account `{}` ({e})", self.address),
            },
        };

        // Construct account from boc to get ecc balance
        let tvm_account = tvm_block::Account::construct_from_base64(&boc)
            .map_err(|e| anyhow!("Construct account `{}` from boc ({e})", self.address))?;

        // Parse account boc
        let parsed = tvm_client::boc::parse_account(
            self.context.clone(),
            ParamsOfParse { boc: boc.clone() },
        )
        .map_err(|e| anyhow!("Parse account `{}` ({e})", self.address))?
        .parsed;

        // Deserialize account value
        let deserialized = serde_json::from_value::<AccountData>(parsed)
            .map_err(|e| anyhow!("Deserialize account `{}` ({e})", self.address))?;

        self.boc = deserialized.boc;
        self.data = deserialized.data;
        self.acc_type = deserialized.acc_type;
        self.balance = deserialized.balance;
        self.ecc = match tvm_account.balance() {
            Some(balance) => {
                let mut map = BTreeMap::new();
                balance
                    .other
                    .iterate_with_keys::<u32, _>(|k, v| {
                        map.insert(k, v.value().clone());
                        Ok(true)
                    })
                    .map_err(|e| anyhow!("Iterate account `{}` currency ({e})", self.address))?;
                map
            }
            None => BTreeMap::new(),
        };

        Ok(())
    }

    pub async fn wait(&mut self, params: ParamsOfWaitAccount) -> anyhow::Result<()> {
        let mut attempts = 0;
        loop {
            if attempts == params.attempts.unwrap_or(20) {
                anyhow::bail!(
                    "Wait for account `{}` status `{:?}`. Max attempts reached.",
                    self.address,
                    params.status,
                );
            }

            self.fetch().await?;
            if self.acc_type == params.status {
                return Ok(());
            }

            attempts += 1;

            let timeout = params.attempts_timeout.unwrap_or(1000);
            tokio::time::sleep(Duration::from_millis(timeout)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use num_bigint::BigInt;
    use shared::traits::guarded::AsyncGuarded;
    use tvm_client::ClientConfig;
    use tvm_client::ClientContext;

    use crate::account::Account;
    use crate::account::AccountStatus;
    use crate::account::ParamsOfWaitAccount;
    use crate::mvsystem::mvmultifactor::MvMultifactor;
    use crate::traits::AccountAccessor;
    use crate::traits::DecodeAccountData;

    fn create_context() -> Arc<ClientContext> {
        let mut config = ClientConfig::default();
        config.network.endpoints = Some(vec!["shellnet.ackinacki.org".to_string()]);

        let context = ClientContext::new(config).expect("Create context");
        Arc::new(context)
    }

    #[tokio::test]
    async fn test_fetch_account() {
        let context = create_context();
        let mut account = Account::new(
            context,
            "0:2222222222222222222222222222222222222222222222222222222222222222",
        );
        let fetch_result = account.fetch().await.inspect_err(|e| eprintln!("Fetch account ({e})"));
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
        );
        let wait_result = account
            .wait(ParamsOfWaitAccount { status: AccountStatus::Active, ..Default::default() })
            .await
            .inspect_err(|e| eprintln!("Wait account ({e})"));
        assert!(wait_result.is_ok());

        // Wait for non existing account
        let mut account = Account::new(
            context.clone(),
            "0:2222222222222222222222222222222222222222222222222222222222222220",
        );
        let wait_result = account
            .wait(ParamsOfWaitAccount {
                status: AccountStatus::Active,
                attempts: Some(3),
                attempts_timeout: Some(500),
            })
            .await
            .inspect_err(|e| eprintln!("Wait account ({e})"));
        assert!(wait_result.is_err());
    }

    #[tokio::test]
    async fn test_decode_account() {
        let context = create_context();

        // Wait for existing account
        let mut account = Account::new(
            context.clone(),
            "0:269840b497d21dc35c73ccfd31158eade4245ba01230196842acd5f8f3655011",
        );
        account.fetch().await.inspect_err(|e| eprintln!("Fetch account ({e})")).unwrap();
    }

    #[tokio::test]
    async fn test_decode_multifactor_account_data() {
        let context = create_context();

        let mvmultifactor = MvMultifactor::new(
            context,
            "0:cb40e80ae77e611738e765cd3979ad438ec8beab1b7099784e161b5c3a71e6d5",
        );
        let fetch = mvmultifactor.fetch_account().await;
        assert!(fetch.is_ok());

        let data = mvmultifactor.async_guarded(|account| account.data.clone()).await.unwrap();
        let decoded = mvmultifactor
            .decode_account_data(data)
            .inspect_err(|e| eprintln!("Decode multifactor data ({e})"))
            .unwrap();
        assert_eq!(decoded.index_mod_4, "1");
    }
}
