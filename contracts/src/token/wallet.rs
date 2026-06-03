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
use crate::deserialize::deserialize_option_u128;
use crate::deserialize::deserialize_u128;
use crate::error::KitModule;
use crate::error::TokenModule;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::GetMethodAccessor;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/token/TokenWallet.abi.json");

#[derive(Debug, Clone)]
pub struct TokenWallet {
    context: Arc<ClientContext>,
    address: String,
    dapp_id: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ModuleAccessor for TokenWallet {
    const MODULE: KitModule = KitModule::Token(TokenModule::Wallet);
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

    fn dapp_id(&self) -> &str {
        &self.dapp_id
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

impl AsyncGuarded<Account> for TokenWallet {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for TokenWallet {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
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

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    pub async fn get_transaction_address(
        &self,
        params: ParamsOfGetTransactionAddress,
    ) -> KitResult<ResultOfGetTransactionAddress> {
        self.call_get_method_with::<ResultOfGetTransactionAddress, ParamsOfGetTransactionAddress>(
            "getTransactionAddress",
            params,
        )
        .await
    }

    pub async fn deploy_transaction(
        &self,
        params: ParamsOfDeployTransaction,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployTransaction".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
