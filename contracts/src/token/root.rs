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

use crate::account::Account;
use crate::delivery::ContractContext;
use crate::deserialize::deserialize_u128;
use crate::error::KitModule;
use crate::error::TokenModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/token/RootToken.abi.json");

#[derive(Debug, Clone)]
pub struct TokenRoot {
    base: ContractBase,
}

impl ModuleAccessor for TokenRoot {
    const MODULE: KitModule = KitModule::Token(TokenModule::Root);
}

impl HasContractBase for TokenRoot {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for TokenRoot {}

impl AsyncGuarded<Account> for TokenRoot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for TokenRoot {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
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
    pub fn new(
        context: impl Into<ContractContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())) }
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
