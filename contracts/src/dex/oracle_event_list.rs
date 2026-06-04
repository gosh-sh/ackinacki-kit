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

const ABI: &str = include_str!("../../abi/dex/OracleEventList.abi.json");

#[derive(Debug, Clone)]
/// Wrapper for an `OracleEventList` shard contract.
pub struct OracleEventList {
    base: ContractBase,
}

impl ModuleAccessor for OracleEventList {
    const MODULE: KitModule = KitModule::Dex(DexModule::OracleEventList);
}

impl HasContractBase for OracleEventList {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for OracleEventList {}

impl AsyncGuarded<Account> for OracleEventList {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for OracleEventList {
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
#[serde(rename_all = "camelCase")]
/// Parameters for `OracleEventList.addEvent`.
pub struct ParamsOfAddEvent {
    pub event_name: String,
    pub oracle_fee: u128,
    pub deadline: u64,
    pub describe: String,
    pub outcome_names: HashMap<u32, String>,
    pub trust_addr: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OracleEventList.deleteEvent`.
pub struct ParamsOfDeleteEvent {
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize)]
/// Parameters for `OracleEventList.setDescription`.
pub struct ParamsOfSetDescription {
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
/// Parameters for `OracleEventList.confirmEvent` and `OracleEventList.cancelEvent`.
pub struct ParamsOfConfirmOrCancelEvent {
    pub event_id: String,
    pub oracle_list_hash: String,
    pub token_type: u32,
}

#[derive(Debug, Clone, Deserialize)]
/// Result of `OracleEventList._events` getter.
///
/// Entries are left as raw JSON because the event tuple schema can evolve.
pub struct ResultOfGetEvents {
    #[serde(rename = "_events")]
    pub events: HashMap<String, serde_json::Value>,
}

impl OracleEventList {
    /// Create a wrapper for a deployed `OracleEventList` shard.
    pub fn new(
        context: Arc<ClientContext>,
        params: impl Into<crate::account::ParamsOfNewContract>,
    ) -> Self {
        let params = params.into();
        Self { base: ContractBase::new(context, params, Abi::Json(ABI.to_string())) }
    }

    /// # Update human-readable list description
    ///
    /// Original contract method: `setDescription`
    ///
    /// Should be signed with oracle owner keys
    pub async fn set_description(
        &self,
        params: ParamsOfSetDescription,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setDescription".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Add oracle-serviced event
    ///
    /// Original contract method: `addEvent`
    ///
    /// Should be signed with oracle owner keys
    pub async fn add_event(
        &self,
        params: ParamsOfAddEvent,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "addEvent".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Delete event from list
    ///
    /// Original contract method: `deleteEvent`
    ///
    /// Should be signed with oracle owner keys
    pub async fn delete_event(
        &self,
        params: ParamsOfDeleteEvent,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deleteEvent".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Confirm event and deploy PMP
    ///
    /// Original contract method: `confirmEvent`
    ///
    /// Should be signed with oracle owner keys
    pub async fn confirm_event(
        &self,
        params: ParamsOfConfirmOrCancelEvent,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "confirmEvent".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Cancel event in PMP
    ///
    /// Original contract method: `cancelEvent`
    ///
    /// Should be signed with oracle owner keys
    pub async fn cancel_event(
        &self,
        params: ParamsOfConfirmOrCancelEvent,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "cancelEvent".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Read full `_events` mapping
    ///
    /// Original contract method: `_events`
    ///
    /// Returns raw JSON values for map entries to keep the wrapper stable while
    /// DEX event tuple schema is still evolving.
    pub async fn get_events(&self) -> KitResult<ResultOfGetEvents> {
        self.call_get_method::<ResultOfGetEvents>("_events").await
    }
}
