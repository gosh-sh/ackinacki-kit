use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use tvm_client::net::OrderBy;
use tvm_client::net::ParamsOfQueryCollection;
use tvm_client::net::{self};
use tvm_client::ClientContext;

use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::traits::DecodeMessage;
use crate::KitResult;

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub id: String,
    pub src: String,
    pub dst: String,
    pub created_at: u64,
    pub boc: String,
}

impl Event {
    pub fn decode<T: DeserializeOwned>(
        &self,
        contract: &impl DecodeMessage,
    ) -> KitResult<Option<T>> {
        let decoded = contract.decode_message(self.boc.clone())?.value;

        if let Some(value) = decoded {
            let deserialized = serde_json::from_value::<T>(value).map_err(|e| {
                KitError::new(
                    KitModule::Event,
                    KitErrorCode::DeserializeFailed,
                    format!("Deserialize message data ({e})"),
                )
            })?;
            Ok(Some(deserialized))
        } else {
            Ok(None)
        }
    }
}

pub async fn query_events(
    context: Arc<ClientContext>,
    filter: Option<serde_json::Value>,
    order: Option<Vec<OrderBy>>,
    limit: Option<u32>,
) -> KitResult<Vec<Event>> {
    let events = net::query_collection(
        context,
        ParamsOfQueryCollection {
            collection: "messages".to_string(),
            filter,
            result: "id src dst created_at boc".to_string(),
            order,
            limit,
        },
    )
    .await
    .map_err(|e| {
        KitError::new(KitModule::Event, KitErrorCode::QueryEvents, format!("Query events ({e})"))
    })?
    .result
    .iter()
    .map(|row| serde_json::from_value::<Event>(row.clone()))
    .collect::<Result<Vec<Event>, _>>()
    .map_err(|e| {
        KitError::new(
            KitModule::Event,
            KitErrorCode::DeserializeFailed,
            format!("Deserialize events ({e})"),
        )
    })?
    .into_iter()
    .filter(|event| event.dst.starts_with(":"))
    .collect::<Vec<_>>();

    Ok(events)
}
