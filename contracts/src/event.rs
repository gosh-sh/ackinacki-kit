use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use tvm_client::net;
use tvm_client::ClientContext;

use crate::account::account_id_from_address;
use crate::dapp::supports_dapp_id;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::traits::DecodeMessage;
use crate::KitResult;

const DEFAULT_PAGE_SIZE: i32 = 100;

/// Legacy (`< 1.0.0`) query — addresses the account by `address`.
const GQL_ACCOUNT_EVENTS_QUERY: &str = r#"
    query($address: String!, $last: Int!, $before: String) {
      blockchain {
        account(address: $address) {
          events(last: $last, before: $before) {
            edges {
              cursor
              node {
                msg_id
                created_at
                dst
                body
              }
            }
          }
        }
      }
    }
"#;

/// v3 (`>= 1.0.0`) query — addresses the account by `account_id` + `dapp_id`.
const GQL_ACCOUNT_EVENTS_QUERY_V3: &str = r#"
    query($account_id: String!, $dapp_id: String!, $last: Int!, $before: String) {
      blockchain {
        account(account_id: $account_id, dapp_id: $dapp_id) {
          events(last: $last, before: $before) {
            edges {
              cursor
              node {
                msg_id
                created_at
                dst
                body
              }
            }
          }
        }
      }
    }
"#;

#[derive(Debug, Clone)]
pub struct Event {
    pub id: String,
    pub dst: String,
    pub created_at: u64,
    pub body: String,
}

impl Event {
    pub fn decode<T: DeserializeOwned>(
        &self,
        contract: &impl DecodeMessage,
    ) -> KitResult<Option<T>> {
        let decoded = contract.decode_message_body(&self.body)?;

        if let Some(value) = decoded.value {
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
    address: &str,
    dapp_id: Option<&str>,
    limit: Option<u32>,
) -> KitResult<Vec<Event>> {
    query_events_while(context, address, dapp_id, limit, |_| true).await
}

pub async fn query_events_while(
    context: Arc<ClientContext>,
    address: &str,
    dapp_id: Option<&str>,
    limit: Option<u32>,
    predicate: impl Fn(&Event) -> bool,
) -> KitResult<Vec<Event>> {
    let page_size = limit.map(|l| l as i32).unwrap_or(DEFAULT_PAGE_SIZE);
    let mut all_events = Vec::new();
    let mut before: Option<String> = None;

    // Pick the wire format once: `>= 1.0.0` servers address the account by
    // `account_id` + `dapp_id`, legacy servers by `address`.
    let v3 = supports_dapp_id(&context, KitModule::Event).await?;
    let query = if v3 { GQL_ACCOUNT_EVENTS_QUERY_V3 } else { GQL_ACCOUNT_EVENTS_QUERY };
    let account_id = account_id_from_address(address);

    loop {
        let variables = if v3 {
            serde_json::json!({
                "account_id": account_id,
                "dapp_id": dapp_id.unwrap_or_default(),
                "last": page_size,
                "before": before,
            })
        } else {
            serde_json::json!({
                "address": address,
                "last": page_size,
                "before": before,
            })
        };
        let raw = net::query(
            context.clone(),
            net::ParamsOfQuery { query: query.to_string(), variables: Some(variables) },
        )
        .await
        .map_err(|e| {
            KitError::new(KitModule::Event, KitErrorCode::QueryEvents, "Query events with GraphQL")
                .with_tvm_error(e)
        })?;

        let parsed: GqlEventsResponse = serde_json::from_value(raw.result).map_err(|e| {
            KitError::new(
                KitModule::Event,
                KitErrorCode::DeserializeFailed,
                format!("Deserialize events GraphQL response ({e})"),
            )
        })?;

        let edges = parsed.data.blockchain.account.events.edges;
        if edges.is_empty() {
            break;
        }

        let next_before = edges.first().map(|edge| edge.cursor.clone());
        let mut stop = false;
        for edge in edges {
            let node = edge.node;
            let event = Event {
                id: node.msg_id,
                dst: node.dst,
                created_at: node.created_at,
                body: node.body,
            };
            if predicate(&event) {
                all_events.push(event);
            } else {
                stop = true;
            }
        }

        if stop || limit.is_some() {
            break;
        }

        match (before.as_ref(), next_before) {
            (_, None) => break,
            (Some(current), Some(next)) if current == &next => break,
            (_, Some(next)) => before = Some(next),
        }
    }

    Ok(all_events)
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEventsResponse {
    data: GqlEventsData,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEventsData {
    blockchain: GqlBlockchain,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlBlockchain {
    account: GqlAccount,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlAccount {
    events: GqlEventsList,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEventsList {
    edges: Vec<GqlEdge>,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEdge {
    cursor: String,
    node: GqlEventNode,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEventNode {
    msg_id: String,
    created_at: u64,
    dst: String,
    body: String,
}
