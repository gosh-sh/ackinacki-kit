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

const ABI: &str = include_str!("../../abi/token/RootToken.abi.json");

#[derive(Debug, Clone)]
pub struct TokenRoot {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for TokenRoot {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for TokenRoot {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for TokenRoot {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for TokenRoot {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl ModuleAccessor for TokenRoot {
    const MODULE: KitModule = KitModule::Token(TokenModule::Root);
}

impl EncodeMessage for TokenRoot {}

impl DecodeMessage for TokenRoot {}

impl Executor for TokenRoot {}

impl SendMessage for TokenRoot {}

impl AsyncGuarded<Account> for TokenRoot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for TokenRoot {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetDetails {
    pub name: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub decimals: u128,
    pub deployer: String,
    #[serde(deserialize_with = "deserialize_u128")]
    pub minted: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub burned: u128,
    #[serde(rename = "mintDisabled")]
    pub mint_disabled: bool,
    #[serde(rename = "ownerPubkey")]
    pub owner_pubkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamsOfGetWalletAddress {
    #[serde(rename = "walletOwner")]
    pub owner_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultOfGetWalletAddress {
    #[serde(rename = "walletAddress")]
    pub wallet_address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeployWallet {
    #[serde(rename(serialize = "owner"))]
    pub owner_address: String,
}

impl TokenRoot {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    pub async fn get_wallet_address(
        &self,
        params: ParamsOfGetWalletAddress,
    ) -> KitResult<ResultOfGetWalletAddress> {
        self.call_get_method_with::<ResultOfGetWalletAddress, ParamsOfGetWalletAddress>(
            "getWalletAddress",
            params,
        )
        .await
    }

    pub async fn deploy_wallet(
        &self,
        params: ParamsOfDeployWallet,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployWallet".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
