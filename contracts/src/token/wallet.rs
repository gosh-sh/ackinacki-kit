use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
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
use crate::deserialize::deserialize_option_u128;
use crate::deserialize::deserialize_u128;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/token/TokenWallet.abi.json");

#[derive(Debug, Clone)]
pub struct TokenWallet {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for TokenWallet {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for TokenWallet {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for TokenWallet {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for TokenWallet {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for TokenWallet {}

impl DecodeMessage for TokenWallet {}

impl Executor for TokenWallet {}

impl SendMessage for TokenWallet {}

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
impl AsyncGuarded<Account> for TokenWallet {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T + 'async_trait,
        T: 'async_trait,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
impl AsyncGuardedMut<Account> for TokenWallet {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut + 'async_trait,
        Fut: Future<Output = anyhow::Result<T>> + 'async_trait,
        T: 'async_trait,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetDetails {
    pub root: String,
    pub owner: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub balance: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(u8)]
#[serde(from = "u8", into = "u8")]
pub enum TransactionType {
    Transfer = 1,
    Burn = 2,
    Destroy = 3,
    Withdraw = 4,
    SetSubscriber = 5,
}

impl From<u8> for TransactionType {
    fn from(value: u8) -> Self {
        match value {
            1 => TransactionType::Transfer,
            2 => TransactionType::Burn,
            3 => TransactionType::Destroy,
            4 => TransactionType::Withdraw,

            _ => panic!("Unknown allowed payload destination {value}"),
        }
    }
}

impl From<TransactionType> for u8 {
    fn from(value: TransactionType) -> Self {
        value as u8
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamsOfGetTransactionAddress {
    #[serde(rename(serialize = "transactionType"))]
    pub transaction_type: TransactionType,
    #[serde(deserialize_with = "deserialize_option_u128")]
    pub value: Option<u128>,
    #[serde(rename = "destinationOwner")]
    pub destination_owner: Option<String>,
    #[serde(rename = "toWithdraw")]
    pub to_withdraw: Option<String>,
}
#[derive(Debug, Clone)]
pub enum Transaction {
    Transfer { value: u128, destination_owner: String },
    Burn { value: u128 },
    Withdraw { value: u128, to_withdraw: String },
    SetSubscriber { destination_owner: Option<String> },
    Destroy,
}

impl From<Transaction> for ParamsOfGetTransactionAddress {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Transfer { value, destination_owner } => Self {
                transaction_type: TransactionType::Transfer,
                value: Some(value),
                destination_owner: Some(destination_owner),
                to_withdraw: None,
            },
            Transaction::Burn { value } => Self {
                transaction_type: TransactionType::Burn,
                value: Some(value),
                destination_owner: None,
                to_withdraw: None,
            },
            Transaction::Withdraw { value, to_withdraw } => Self {
                transaction_type: TransactionType::Withdraw,
                value: Some(value),
                destination_owner: None,
                to_withdraw: Some(to_withdraw),
            },
            Transaction::SetSubscriber { destination_owner } => Self {
                transaction_type: TransactionType::SetSubscriber,
                value: None,
                destination_owner,
                to_withdraw: None,
            },
            Transaction::Destroy => Self {
                transaction_type: TransactionType::Destroy,
                value: None,
                destination_owner: None,
                to_withdraw: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetTransactionAddress {
    #[serde(rename = "transactionAddress")]
    pub transaction_address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeployTransaction {
    #[serde(rename(serialize = "transactionType"))]
    pub transaction_type: TransactionType,
    #[serde(deserialize_with = "deserialize_option_u128")]
    pub value: Option<u128>,
    #[serde(rename = "destinationOwner")]
    pub destination_owner: Option<String>,
    #[serde(rename = "toWithdraw")]
    pub to_withdraw: Option<String>,
}

impl From<Transaction> for ParamsOfDeployTransaction {
    fn from(tx: Transaction) -> Self {
        match tx {
            Transaction::Transfer { value, destination_owner } => Self {
                transaction_type: TransactionType::Transfer,
                value: Some(value),
                destination_owner: Some(destination_owner),
                to_withdraw: None,
            },
            Transaction::Burn { value } => Self {
                transaction_type: TransactionType::Burn,
                value: Some(value),
                destination_owner: None,
                to_withdraw: None,
            },
            Transaction::Withdraw { value, to_withdraw } => Self {
                transaction_type: TransactionType::Withdraw,
                value: Some(value),
                destination_owner: None,
                to_withdraw: Some(to_withdraw),
            },
            Transaction::SetSubscriber { destination_owner } => Self {
                transaction_type: TransactionType::SetSubscriber,
                value: None,
                destination_owner,
                to_withdraw: None,
            },
            Transaction::Destroy => Self {
                transaction_type: TransactionType::Destroy,
                value: None,
                destination_owner: None,
                to_withdraw: None,
            },
        }
    }
}

impl TokenWallet {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    pub async fn get_details(&self) -> anyhow::Result<ResultOfGetDetails> {
        let call_set =
            CallSet { function_name: "getDetails".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetDetails>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    pub async fn get_transaction_address(
        &self,
        params: ParamsOfGetTransactionAddress,
    ) -> anyhow::Result<ResultOfGetTransactionAddress> {
        let call_set = CallSet {
            function_name: "getTransactionAddress".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetTransactionAddress>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    pub async fn deploy_transaction(
        &self,
        params: ParamsOfDeployTransaction,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployTransaction".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
