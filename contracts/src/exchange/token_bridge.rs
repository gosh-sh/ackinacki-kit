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
use crate::deserialize::deserialize_u128;
use crate::deserialize::deserialize_u64;
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

const ABI: &str = include_str!("../../abi/exchange/TokenBridge.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for `TokenBridge` contract.
pub struct TokenBridge {
    base: ContractBase,
}

impl ModuleAccessor for TokenBridge {
    const MODULE: KitModule = KitModule::Exchange(ExchangeModule::TokenBridge);
}

impl HasContractBase for TokenBridge {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for TokenBridge {}

impl AsyncGuarded<Account> for TokenBridge {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for TokenBridge {
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
/// Parameters for `TokenBridge.mintAndSend`.
pub struct ParamsOfMintAndSend {
    pub recipient: String,
    pub value: u128,
    pub nonce: u64,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `TokenBridge.mintAndSendAccumulator`.
pub struct ParamsOfMintAndSendAccumulator {
    pub buyer: String,
    pub value: u128,
    pub nonce: u64,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `TokenBridge.initiateWithdrawal`.
pub struct ParamsOfInitiateWithdrawal {
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "dstChainId"))]
    pub dst_chain_id: String,
    /// `bytes` payload encoded as hex.
    pub recipient: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `TokenBridge.finalizeDeposit`.
pub struct ParamsOfFinalizeDeposit {
    /// `bytes` payload encoded as hex.
    pub proof: String,
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "srcDappId"))]
    pub src_dapp_id: String,
    /// `bytes` payload encoded as hex.
    #[serde(rename(serialize = "srcSender"))]
    pub src_sender: String,
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "recipient_an"))]
    pub recipient_an: String,
    pub amount: u128,
    #[serde(rename(serialize = "tokenId"))]
    pub token_id: u32,
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "srcDepositId"))]
    pub src_deposit_id: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `TokenBridge.confirmDeposit`.
pub struct ParamsOfConfirmDeposit {
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "srcDappId"))]
    pub src_dapp_id: String,
    /// `bytes` payload encoded as hex.
    #[serde(rename(serialize = "srcSender"))]
    pub src_sender: String,
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "recipient_an"))]
    pub recipient_an: String,
    pub amount: u128,
    #[serde(rename(serialize = "tokenId"))]
    pub token_id: u32,
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "srcDepositId"))]
    pub src_deposit_id: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `TokenBridge.setPubkey`.
pub struct ParamsOfSetPubkey {
    /// `uint256` encoded as decimal or hex string.
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `TokenBridge.triggerTransaction`.
pub struct ParamsOfTriggerTransaction {
    #[serde(rename(serialize = "txAddr"))]
    pub tx_addr: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `TokenBridge.updateCode`.
pub struct ParamsOfUpdateCode {
    pub newcode: String,
    #[serde(rename(serialize = "userCell"))]
    pub user_cell: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `TokenBridge.getTotalBridged`.
pub struct ParamsOfGetTotalBridged {
    #[serde(rename(serialize = "tokenId"))]
    pub token_id: u32,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `TokenBridge.getUsdcWallet`.
pub struct ResultOfGetUsdcWallet {
    #[serde(rename = "value0")]
    pub usdc_wallet: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `TokenBridge.getOwnerPubkey`.
pub struct ResultOfGetOwnerPubkey {
    #[serde(rename = "value0")]
    pub owner_pubkey: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `TokenBridge.getTotalMinted`.
pub struct ResultOfGetTotalMinted {
    #[serde(rename = "value0", deserialize_with = "deserialize_u128")]
    pub total_minted: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `TokenBridge.getTotalBridged`.
pub struct ResultOfGetTotalBridged {
    #[serde(deserialize_with = "deserialize_u128")]
    pub minted: u128,
    #[serde(deserialize_with = "deserialize_u128")]
    pub burned: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `TokenBridge.getDepositVoucherCodeHash`. `uint256` returned as hex string.
pub struct ResultOfGetDepositVoucherCodeHash {
    #[serde(rename = "value0")]
    pub code_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `TokenBridge.getNonces`.
pub struct ResultOfGetNonces {
    #[serde(rename = "mintNonce", deserialize_with = "deserialize_u64")]
    pub mint_nonce: u64,
    #[serde(rename = "mintAccumulatorNonce", deserialize_with = "deserialize_u64")]
    pub mint_accumulator_nonce: u64,
}

impl TokenBridge {
    /// Create wrapper for deployed `TokenBridge` contract.
    pub fn new(
        context: Arc<ClientContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        let params = params.into();
        Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())) }
    }

    /// Wrapper bound to `address`, under the all-zero system dApp.
    pub fn new_default(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self::new(
            context,
            crate::account::ParamsOfNewContract::new(
                address.as_ref(),
                crate::dapp::SystemDapp::System,
            ),
        )
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

    /// Original contract method: `initiateWithdrawal`.
    pub async fn initiate_withdrawal(
        &self,
        params: ParamsOfInitiateWithdrawal,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "initiateWithdrawal".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `finalizeDeposit`.
    pub async fn finalize_deposit(
        &self,
        params: ParamsOfFinalizeDeposit,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "finalizeDeposit".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// Original contract method: `confirmDeposit`.
    pub async fn confirm_deposit(
        &self,
        params: ParamsOfConfirmDeposit,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "confirmDeposit".to_string(),
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

    /// Original contract method: `getTotalMinted`.
    pub async fn get_total_minted(&self) -> KitResult<ResultOfGetTotalMinted> {
        self.call_get_method::<ResultOfGetTotalMinted>("getTotalMinted").await
    }

    /// Original contract method: `getTotalBridged`.
    pub async fn get_total_bridged(
        &self,
        params: ParamsOfGetTotalBridged,
    ) -> KitResult<ResultOfGetTotalBridged> {
        self.call_get_method_with::<ResultOfGetTotalBridged, ParamsOfGetTotalBridged>(
            "getTotalBridged",
            params,
        )
        .await
    }

    /// Original contract method: `getDepositVoucherCodeHash`.
    pub async fn get_deposit_voucher_code_hash(
        &self,
    ) -> KitResult<ResultOfGetDepositVoucherCodeHash> {
        self.call_get_method::<ResultOfGetDepositVoucherCodeHash>("getDepositVoucherCodeHash").await
    }

    /// Original contract method: `getNonces`.
    pub async fn get_nonces(&self) -> KitResult<ResultOfGetNonces> {
        self.call_get_method::<ResultOfGetNonces>("getNonces").await
    }
}
