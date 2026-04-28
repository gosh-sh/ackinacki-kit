use std::collections::HashMap;
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
use crate::deserialize::deserialize_u128_map;
use crate::error::DexModule;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AutoContract;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/dex/PrivateNote.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for the DEX `PrivateNote` contract.
pub struct PrivateNote {
    base: ContractBase,
}

impl ModuleAccessor for PrivateNote {
    const MODULE: KitModule = KitModule::Dex(DexModule::PrivateNote);
}

impl HasContractBase for PrivateNote {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for PrivateNote {}

impl AsyncGuarded<Account> for PrivateNote {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for PrivateNote {
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
/// Parameters for `PrivateNote.changeOwner`.
pub struct ParamsOfChangeOwner {
    #[serde(rename(serialize = "newPubkey"))]
    pub new_pubkey: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `PrivateNote.deployPMP`.
pub struct ParamsOfDeployPmp {
    pub event_id: String,
    pub oracle_fee: Vec<u128>,
    pub token_type: u32,
    pub names: Vec<String>,
    pub index: Vec<u128>,
    pub initial_stakes: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Shared PMP key (`event_id`, `oracle_list_hash`, `token_type`) used by
/// multiple `PrivateNote` methods (`deleteStake`, `cancelStake`, `claim`).
pub struct ParamsOfStakeKey {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.withdrawFullSet`.
pub struct ParamsOfWithdrawFullSet {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub amount: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onInitialStakesAccepted`.
pub struct ParamsOfOnInitialStakesAccepted {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub amounts: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onInitialStakesFailed`.
pub struct ParamsOfOnInitialStakesFailed {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    #[serde(rename(serialize = "refundTotal"))]
    pub refund_total: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onStakeCancelled`.
pub struct ParamsOfOnStakeCancelled {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub value: u128,
    pub coupon_value: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onFullSetStakeCancelled`.
pub struct ParamsOfOnFullSetStakeCancelled {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub amount: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.splitFullSet`.
pub struct ParamsOfSplitFullSet {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub collateral: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onSplitAccepted`.
pub struct ParamsOfOnSplitAccepted {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub amounts: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onMergeAccepted`.
pub struct ParamsOfOnMergeAccepted {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub collateral: u128,
    pub amounts: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onStakeAccepted`.
pub struct ParamsOfOnStakeAccepted {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub outcome_count: u128,
    pub bet_type: u8,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onFullSetStakeAccepted`.
pub struct ParamsOfOnFullSetStakeAccepted {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub amount: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onClaimAccepted`.
pub struct ParamsOfOnClaimAccepted {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub outcome: Option<u32>,
    #[serde(rename(serialize = "payoutClean"))]
    pub payout_clean: u128,
    pub payout_debt: u128,
    pub payout_coupon: u128,
    #[serde(rename(serialize = "debtPaid"))]
    pub debt_paid: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.acceptFee`.
pub struct ParamsOfAcceptFee {
    pub fee: u128,
    pub token_type: u32,
    pub event_id: String,
    pub oracle_list_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `PrivateNote.setStake`.
pub struct ParamsOfSetStake {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub outcome: u32,
    pub amount: u128,
    pub use_coupon: bool,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.setFullSetStake`.
pub struct ParamsOfSetFullSetStake {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    pub amount: Vec<u128>,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.generateCoupon`.
pub struct ParamsOfGenerateCoupon {
    pub token_type: u32,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.initTransfer`.
pub struct ParamsOfInitTransfer {
    #[serde(rename(serialize = "destDepositHash"))]
    pub dest_deposit_hash: String,
    #[serde(rename(serialize = "tokenType"))]
    pub token_type: u32,
    pub amount: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.offerTransfer`.
pub struct ParamsOfOfferTransfer {
    pub token_type: u32,
    pub amount: u128,
    pub sender_deposit_hash: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.withdrawTokens`.
pub struct ParamsOfWithdrawTokens {
    #[serde(rename(serialize = "destWalletAddr"))]
    pub dest_wallet_addr: String,
    #[serde(rename(serialize = "tokenType"))]
    pub token_type: u32,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.revertWithdraw`.
pub struct ParamsOfRevertWithdraw {
    pub token_type: u32,
    pub value: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.placeOrder`.
pub struct ParamsOfPlaceOrder {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    #[serde(rename(serialize = "outcomeId"))]
    pub outcome_id: u32,
    #[serde(rename(serialize = "isBuy"))]
    pub is_buy: bool,
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "priceBps"))]
    pub price_bps: String,
    pub amount: u128,
    pub flags: u8,
    #[serde(rename(serialize = "minAmount"))]
    pub min_amount: u128,
    #[serde(rename(serialize = "epochId"))]
    pub epoch_id: u64,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onOrderPlaced` and `PrivateNote.cancelOrder`.
pub struct ParamsOfOrderId {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    #[serde(rename(serialize = "orderId"))]
    pub order_id: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onOrderCancelled`.
pub struct ParamsOfOnOrderCancelled {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    #[serde(rename(serialize = "orderId"))]
    pub order_id: u128,
    #[serde(rename(serialize = "outcomeId"))]
    pub outcome_id: u32,
    #[serde(rename(serialize = "isBuy"))]
    pub is_buy: bool,
    pub amount: u128,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `PrivateNote.onOrderFilled`.
pub struct ParamsOfOnOrderFilled {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
    #[serde(rename(serialize = "outcomeId"))]
    pub outcome_id: u32,
    #[serde(rename(serialize = "filledAmount"))]
    pub filled_amount: u128,
    /// `uint256` encoded as decimal or hex string.
    #[serde(rename(serialize = "clearingPrice"))]
    pub clearing_price: String,
    #[serde(rename(serialize = "isBuy"))]
    pub is_buy: bool,
    #[serde(rename(serialize = "refundAmount"))]
    pub refund_amount: u128,
    #[serde(rename(serialize = "feeAmount"))]
    pub fee_amount: u128,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PrivateNote.getPMPCode`.
pub struct ResultOfGetPmpCode {
    #[serde(rename = "pmpCode")]
    pub pmp_code: String,
    #[serde(rename = "pmpCodeHash")]
    pub pmp_code_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PrivateNote.getDetails`.
pub struct ResultOfGetDetails {
    #[serde(rename = "depositIdentifierHash")]
    pub deposit_identifier_hash: String,
    #[serde(rename = "ephemeralPubkey")]
    pub ephemeral_pubkey: String,
    #[serde(deserialize_with = "deserialize_u128_map")]
    pub balance: HashMap<String, u128>,
    #[serde(rename = "pmpCodeHash")]
    pub pmp_code_hash: String,
    #[serde(rename = "privateNoteCodeHash")]
    pub private_note_code_hash: String,
    #[serde(rename = "busyAddress")]
    pub busy_address: Option<String>,
    #[serde(rename = "couponsValue", deserialize_with = "deserialize_u128")]
    pub coupons_value: u128,
    #[serde(rename = "hasWithdrawn")]
    pub has_withdrawn: bool,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PrivateNote._deposit_identifier_hash`.
pub struct ResultOfGetDepositIdentifierHash {
    #[serde(rename = "_deposit_identifier_hash")]
    pub deposit_identifier_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `PrivateNote._stakes`.
///
/// Stake entries are intentionally kept as raw JSON to keep the wrapper stable
/// across DEX stake tuple schema changes.
pub struct ResultOfGetStakes {
    #[serde(rename = "_stakes")]
    pub stakes: HashMap<String, serde_json::Value>,
}

impl PrivateNote {
    /// Create a wrapper for a deployed `PrivateNote`.
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// # Change ephemeral owner key
    ///
    /// Original contract method: `changeOwner`
    ///
    /// Should be signed with current ephemeral owner keys
    pub async fn change_owner(
        &self,
        params: ParamsOfChangeOwner,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "changeOwner".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy PMP
    ///
    /// Original contract method: `deployPMP`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn deploy_pmp(
        &self,
        params: ParamsOfDeployPmp,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployPMP".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for accepted initial full-set stakes
    ///
    /// Original contract method: `onInitialStakesAccepted`
    pub async fn on_initial_stakes_accepted(
        &self,
        params: ParamsOfOnInitialStakesAccepted,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onInitialStakesAccepted".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for failed initial full-set stakes
    ///
    /// Original contract method: `onInitialStakesFailed`
    pub async fn on_initial_stakes_failed(
        &self,
        params: ParamsOfOnInitialStakesFailed,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onInitialStakesFailed".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Delete stake record
    ///
    /// Original contract method: `deleteStake`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn delete_stake(
        &self,
        params: ParamsOfStakeKey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deleteStake".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Cancel stake on PMP
    ///
    /// Original contract method: `cancelStake`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn cancel_stake(
        &self,
        params: ParamsOfStakeKey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "cancelStake".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for canceled stake
    ///
    /// Original contract method: `onStakeCancelled`
    pub async fn on_stake_cancelled(
        &self,
        params: ParamsOfOnStakeCancelled,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onStakeCancelled".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Withdraw full-set stake from PMP
    ///
    /// Original contract method: `withdrawFullSet`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn withdraw_full_set(
        &self,
        params: ParamsOfWithdrawFullSet,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "withdrawFullSet".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for canceled full-set stake
    ///
    /// Original contract method: `onFullSetStakeCancelled`
    pub async fn on_full_set_stake_cancelled(
        &self,
        params: ParamsOfOnFullSetStakeCancelled,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onFullSetStakeCancelled".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Split full set on PMP
    ///
    /// Original contract method: `splitFullSet`
    pub async fn split_full_set(
        &self,
        params: ParamsOfSplitFullSet,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "splitFullSet".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for accepted split
    ///
    /// Original contract method: `onSplitAccepted`
    pub async fn on_split_accepted(
        &self,
        params: ParamsOfOnSplitAccepted,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onSplitAccepted".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Merge full set on PMP
    ///
    /// Original contract method: `mergeFullSet`
    pub async fn merge_full_set(
        &self,
        params: ParamsOfWithdrawFullSet,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "mergeFullSet".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for accepted merge
    ///
    /// Original contract method: `onMergeAccepted`
    pub async fn on_merge_accepted(
        &self,
        params: ParamsOfOnMergeAccepted,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onMergeAccepted".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Place a single-outcome stake
    ///
    /// Original contract method: `setStake`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn set_stake(
        &self,
        params: ParamsOfSetStake,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setStake".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Place a full-set stake
    ///
    /// Original contract method: `setFullSetStake`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn set_full_set_stake(
        &self,
        params: ParamsOfSetFullSetStake,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setFullSetStake".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for accepted stake
    ///
    /// Original contract method: `onStakeAccepted`
    pub async fn on_stake_accepted(
        &self,
        params: ParamsOfOnStakeAccepted,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onStakeAccepted".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for accepted full-set stake
    ///
    /// Original contract method: `onFullSetStakeAccepted`
    pub async fn on_full_set_stake_accepted(
        &self,
        params: ParamsOfOnFullSetStakeAccepted,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onFullSetStakeAccepted".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Claim PMP payout
    ///
    /// Original contract method: `claim`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn claim(
        &self,
        params: ParamsOfStakeKey,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "claim".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for accepted claim
    ///
    /// Original contract method: `onClaimAccepted`
    pub async fn on_claim_accepted(
        &self,
        params: ParamsOfOnClaimAccepted,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onClaimAccepted".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Accept creator fee from PMP
    ///
    /// Original contract method: `acceptFee`
    pub async fn accept_fee(
        &self,
        params: ParamsOfAcceptFee,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "acceptFee".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Generate coupon
    ///
    /// Original contract method: `generateCoupon`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn generate_coupon(
        &self,
        params: ParamsOfGenerateCoupon,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "generateCoupon".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Initiate transfer to another PrivateNote
    ///
    /// Original contract method: `initTransfer`
    pub async fn init_transfer(
        &self,
        params: ParamsOfInitTransfer,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "initTransfer".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Receive transfer offer callback
    ///
    /// Original contract method: `offerTransfer`
    pub async fn offer_transfer(
        &self,
        params: ParamsOfOfferTransfer,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "offerTransfer".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Confirm accepted transfer callback
    ///
    /// Original contract method: `onTransferAccepted`
    pub async fn on_transfer_accepted(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set =
            CallSet { function_name: "onTransferAccepted".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Clear busy transfer state
    ///
    /// Original contract method: `clearTransferBusy`
    pub async fn clear_transfer_busy(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set =
            CallSet { function_name: "clearTransferBusy".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Discard unused coupon
    ///
    /// Original contract method: `discardCoupon`
    pub async fn discard_coupon(&self, signer: Signer) -> KitResult<ResultOfSendMessage> {
        let call_set =
            CallSet { function_name: "discardCoupon".to_string(), header: None, input: None };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Withdraw tokens via RootPN vault
    ///
    /// Original contract method: `withdrawTokens`
    ///
    /// Should be signed with PrivateNote owner keys
    pub async fn withdraw_tokens(
        &self,
        params: ParamsOfWithdrawTokens,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "withdrawTokens".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Revert token withdraw callback
    ///
    /// Original contract method: `revertWithdraw`
    pub async fn revert_withdraw(
        &self,
        params: ParamsOfRevertWithdraw,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "revertWithdraw".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Place order on PMP order book
    ///
    /// Original contract method: `placeOrder`
    pub async fn place_order(
        &self,
        params: ParamsOfPlaceOrder,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "placeOrder".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for placed order
    ///
    /// Original contract method: `onOrderPlaced`
    pub async fn on_order_placed(
        &self,
        params: ParamsOfOrderId,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onOrderPlaced".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Cancel order on PMP order book
    ///
    /// Original contract method: `cancelOrder`
    pub async fn cancel_order(
        &self,
        params: ParamsOfOrderId,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "cancelOrder".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for canceled order
    ///
    /// Original contract method: `onOrderCancelled`
    pub async fn on_order_cancelled(
        &self,
        params: ParamsOfOnOrderCancelled,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onOrderCancelled".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Process callback for filled order
    ///
    /// Original contract method: `onOrderFilled`
    pub async fn on_order_filled(
        &self,
        params: ParamsOfOnOrderFilled,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "onOrderFilled".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Get salted PMP code and hash
    ///
    /// Original contract method: `getPMPCode`
    pub async fn get_pmp_code(&self) -> KitResult<ResultOfGetPmpCode> {
        self.call_get_method::<ResultOfGetPmpCode>("getPMPCode").await
    }

    /// # Get PrivateNote details
    ///
    /// Original contract method: `getDetails`
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// # Get deposit identifier hash (public static field getter)
    ///
    /// Original contract method: `_deposit_identifier_hash`
    pub async fn get_deposit_identifier_hash(&self) -> KitResult<ResultOfGetDepositIdentifierHash> {
        self.call_get_method::<ResultOfGetDepositIdentifierHash>("_deposit_identifier_hash").await
    }

    /// # Get raw `_stakes` mapping
    ///
    /// Original contract method: `_stakes`
    ///
    /// Returns raw JSON entries because stake tuple schema is large and evolves
    /// frequently in DEX contract iterations.
    pub async fn get_stakes(&self) -> KitResult<ResultOfGetStakes> {
        self.call_get_method::<ResultOfGetStakes>("_stakes").await
    }
}
