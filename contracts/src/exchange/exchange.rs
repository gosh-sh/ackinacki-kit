use std::sync::Arc;

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
use tvm_client::ClientContext;

use crate::account::Account;
use crate::error::ExchangeModule;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/exchange/Exchange.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for `Exchange` contract.
pub struct Exchange {
    base: ContractBase,
}

impl ModuleAccessor for Exchange {
    const MODULE: KitModule = KitModule::Exchange(ExchangeModule::Exchange);
}

impl HasContractBase for Exchange {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for Exchange {}

impl AsyncGuarded<Account> for Exchange {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for Exchange {
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
/// Parameters for `Exchange.mintAndSend`.
pub struct ParamsOfMintAndSend {
    pub recipient: String,
    pub value: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `Exchange.mintAndSendAccumulator`.
pub struct ParamsOfMintAndSendAccumulator {
    #[serde(rename(serialize = "buyer"))]
    pub recipient: String,
    pub value: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `Exchange.setPubkey`.
pub struct ParamsOfSetPubkey {
    /// `uint256` encoded as decimal or hex string.
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `Exchange.triggerTransaction`.
pub struct ParamsOfTriggerTransaction {
    #[serde(rename(serialize = "txAddr"))]
    pub tx_addr: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `Exchange.updateCode`.
pub struct ParamsOfUpdateCode {
    pub newcode: String,
    pub cell: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `Exchange.getUsdcWallet`.
pub struct ResultOfGetUsdcWallet {
    #[serde(rename = "value0")]
    pub usdc_wallet: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `Exchange.getOwnerPubkey`.
pub struct ResultOfGetOwnerPubkey {
    #[serde(rename = "value0")]
    pub owner_pubkey: String,
}

impl Exchange {
    /// Default zerostate exchange address.
    pub const DEFAULT_ADDRESS: &'static str =
        "0:1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a";

    /// Create wrapper for deployed `Exchange` contract.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// Create wrapper bound to default zerostate `Exchange` address.
    pub fn new_default(context: Arc<ClientContext>) -> Self {
        Self::new(context, Self::DEFAULT_ADDRESS)
    }

    /// Original contract method: `mintAndSend`.
    pub async fn mint_and_send(
        &self,
        params: ParamsOfMintAndSend,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "mintAndSend".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `mintAndSendAccumulator`.
    pub async fn mint_and_send_accumulator(
        &self,
        params: ParamsOfMintAndSendAccumulator,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "mintAndSendAccumulator".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `setPubkey`.
    pub async fn set_pubkey(
        &self,
        params: ParamsOfSetPubkey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setPubkey".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `triggerTransaction`.
    pub async fn trigger_transaction(
        &self,
        params: ParamsOfTriggerTransaction,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "triggerTransaction".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `updateCode`.
    pub async fn update_code(
        &self,
        params: ParamsOfUpdateCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "updateCode".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `getUsdcWallet`.
    pub async fn get_usdc_wallet(&self) -> KitResult<ResultOfGetUsdcWallet> {
        self.call_get_method::<ResultOfGetUsdcWallet>("getUsdcWallet").await
    }

    /// Original contract method: `getOwnerPubkey`.
    pub async fn get_owner_pubkey(&self) -> KitResult<ResultOfGetOwnerPubkey> {
        self.call_get_method::<ResultOfGetOwnerPubkey>("getOwnerPubkey").await
    }
}

#[cfg(test)]
mod tests {
    use super::Exchange;

    #[test]
    fn default_address_is_expected_exchange_zerostate() {
        assert_eq!(
            Exchange::DEFAULT_ADDRESS,
            "0:1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a"
        );
    }
}
