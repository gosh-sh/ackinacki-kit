use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::AbiParam;
use tvm_client::abi::CallSet;
use tvm_client::abi::ParamsOfAbiEncodeBoc;
use tvm_client::abi::ParamsOfDecodeBoc;
use tvm_client::abi::ParamsOfDecodeMessageBody;
use tvm_client::abi::Signer;
use tvm_client::net;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::error::AuthServiceModule;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::AutoContract;
use crate::traits::ContextAccessor;
use crate::traits::ContractBase;
use crate::traits::GetMethodAccessor;
use crate::traits::HasContractBase;
use crate::traits::ModuleAccessor;
use crate::traits::SendMessage;
use crate::KitResult;

const ABI: &str = include_str!("../../abi/authservice/AuthProfile.abi.json");

#[derive(Debug, Clone)]
pub struct AuthProfile {
    base: ContractBase,
}

impl ModuleAccessor for AuthProfile {
    const MODULE: KitModule = KitModule::AuthService(AuthServiceModule::Profile);
}

impl HasContractBase for AuthProfile {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for AuthProfile {}

impl AsyncGuarded<Account> for AuthProfile {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for AuthProfile {
    async fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> Result<T, E>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let guard = self.account().clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetDetails {
    pub description: String,
    #[serde(rename = "descriptionHash")]
    pub description_hash: String,
    #[serde(rename = "pubkeyHash")]
    pub pubkey_hash: String,
    #[serde(rename = "multifactorHash")]
    pub multifactor_hash: String,
    pub root: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ParamsOfAddContext {
    pub context: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfQueryProfileEvents {
    /// Lower bound (inclusive) for message timestamp in seconds.
    pub created_at_from: Option<u64>,
    /// Maximum number of messages to fetch per GraphQL query.
    pub limit: Option<u32>,
    /// Reverse-pagination cursor (`before`) for GraphQL account.messages query.
    pub before: Option<String>,
}

impl Default for ParamsOfQueryProfileEvents {
    fn default() -> Self {
        Self { created_at_from: None, limit: Some(50), before: None }
    }
}

#[derive(Debug, Clone)]
pub enum DecodedAuthProfileEvent {
    ContextAdded { event: crate::event::Event, data: ContextAddedTextData },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextAddedTextData {
    /// Original text payload passed by `add_context_text(...)`.
    pub text: String,
}

/// Convenience record for `ContextAdded` profile history entries.
///
/// This is a sugar type over `DecodedAuthProfileEvent::ContextAdded` so callers
/// don't need to match the enum manually when they only care about context
/// additions.
#[derive(Debug, Clone)]
pub struct ContextAddedEventRecord {
    pub event: crate::event::Event,
    pub data: ContextAddedTextData,
}

#[derive(Debug, Clone, Deserialize)]
struct ContextAddedRawCellData {
    context: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlMessagesResponse {
    data: GqlMessagesData,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlMessagesData {
    blockchain: GqlBlockchain,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlBlockchain {
    account: GqlAccount,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlAccount {
    events: GqlEvents,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEvents {
    edges: Vec<GqlEdge>,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEdge {
    node: GqlEventNode,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEventNode {
    #[serde(rename = "msg_id")]
    msg_id: String,
    created_at: u64,
    dst: String,
    body: String,
}

const GQL_PROFILE_EVENTS_QUERY: &str = r#"
    query($address: String!, $dst: String!) {
      blockchain {
        account(address: $address) {
          events(dst: $dst) {
            edges {
              node {
                msg_id
                dst
                created_at
                body
              }
            }
          }
        }
      }
    }
"#;

impl AuthProfile {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    /// # Get profile details
    ///
    /// Original contract method: `getDetails`
    pub async fn get_details(&self) -> KitResult<ResultOfGetDetails> {
        self.call_get_method::<ResultOfGetDetails>("getDetails").await
    }

    /// # Add context
    ///
    /// Original contract method: `addContext`
    ///
    /// Should be signed with profile owner keys.
    /// The `context` payload is a BOC-encoded TVM cell represented as base64 string.
    pub async fn add_context(
        &self,
        params: ParamsOfAddContext,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "addContext".to_string(),
            header: None,
            input: Some(serde_json::json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Encode plain text into TVM cell for `addContext`
    ///
    /// Encodes a regular UTF-8 string into a TVM cell using the same ABI packing
    /// rules as Solidity `TvmBuilder.store(string)`, and returns the result as
    /// base64 BOC string accepted by `addContext`.
    pub fn encode_context_text_cell(&self, text: impl AsRef<str>) -> KitResult<String> {
        let encoded = tvm_client::abi::encode_boc(
            self.context().clone(),
            ParamsOfAbiEncodeBoc {
                params: vec![AbiParam {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    ..Default::default()
                }],
                data: json!({ "value": text.as_ref() }),
                boc_cache: None,
            },
        )
        .map_err(|e| {
            KitError::new(
                KitModule::AuthService(AuthServiceModule::Profile),
                KitErrorCode::None,
                "Encode addContext text into cell",
            )
            .with_tvm_error(e)
        })?;

        Ok(encoded.boc)
    }

    /// # Add text context
    ///
    /// Convenience wrapper over `addContext(TvmCell)` that accepts plain UTF-8
    /// text, encodes it into a TVM cell, and sends the message.
    pub async fn add_context_text(
        &self,
        text: impl AsRef<str>,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let context = self.encode_context_text_cell(text)?;
        self.add_context(ParamsOfAddContext { context }, signer).await
    }

    /// # Query profile events (raw GraphQL + decoded payloads)
    ///
    /// Developer-friendly helper for AuthService profile history. It searches
    /// outbound external messages emitted by the profile (currently `ContextAdded`)
    /// and returns decoded payloads where `ContextAdded.context` is already
    /// decoded from TVM `cell` into plain text.
    pub async fn query_profile_events(
        &self,
        params: ParamsOfQueryProfileEvents,
    ) -> KitResult<Vec<DecodedAuthProfileEvent>> {
        let limit = params.limit.unwrap_or(50) as usize;
        let expected_dst = profile_internal_to_external_address(self.address());
        let variables = json!({
            "address": self.address(),
            "dst": expected_dst,
        });

        let raw = net::query(
            self.context().clone(),
            net::ParamsOfQuery {
                query: GQL_PROFILE_EVENTS_QUERY.to_string(),
                variables: Some(variables),
            },
        )
        .await
        .map_err(|e| {
            KitError::new(
                KitModule::AuthService(AuthServiceModule::Profile),
                KitErrorCode::QueryEvents,
                "Query AuthProfile events with GraphQL",
            )
            .with_tvm_error(e)
        })?;

        let parsed: GqlMessagesResponse = serde_json::from_value(raw.result).map_err(|e| {
            KitError::new(
                KitModule::AuthService(AuthServiceModule::Profile),
                KitErrorCode::DeserializeFailed,
                format!("Deserialize AuthProfile GraphQL response ({e})"),
            )
        })?;

        let mut result = Vec::new();
        for event_node in parsed.data.blockchain.account.events.edges.into_iter().map(|e| e.node) {
            if event_node.created_at < params.created_at_from.unwrap_or_default() {
                continue;
            }

            let decoded = tvm_client::abi::decode_message_body(
                self.context().clone(),
                ParamsOfDecodeMessageBody {
                    abi: self.abi().clone(),
                    body: event_node.body.clone(),
                    is_internal: false,
                    allow_partial: true,
                    function_name: None,
                    data_layout: None,
                },
            )
            .map_err(|e| {
                KitError::new(
                    KitModule::AuthService(AuthServiceModule::Profile),
                    KitErrorCode::Decode,
                    "Decode AuthProfile event body",
                )
                .with_tvm_error(e)
            })?;
            if decoded.name != "ContextAdded" {
                continue;
            }

            let raw_value = decoded.value.ok_or_else(|| {
                KitError::new(
                    KitModule::AuthService(AuthServiceModule::Profile),
                    KitErrorCode::EmptyData,
                    "Empty ContextAdded payload",
                )
            })?;

            let raw_data: ContextAddedRawCellData =
                serde_json::from_value(raw_value).map_err(|e| {
                    KitError::new(
                        KitModule::AuthService(AuthServiceModule::Profile),
                        KitErrorCode::DeserializeFailed,
                        format!("Deserialize ContextAdded raw payload ({e})"),
                    )
                })?;

            let text = self.decode_context_text_cell(&raw_data.context)?;
            let event = crate::event::Event {
                id: event_node.msg_id,
                src: self.address().to_string(),
                dst: event_node.dst,
                created_at: event_node.created_at,
                boc: event_node.body,
            };
            result.push(DecodedAuthProfileEvent::ContextAdded {
                event,
                data: ContextAddedTextData { text },
            });
            if result.len() >= limit {
                break;
            }
        }

        Ok(result)
    }

    /// # Query only `ContextAdded` events
    ///
    /// Convenience wrapper over `query_profile_events(...)` that returns a flat
    /// list of `ContextAdded` records without enum matching.
    pub async fn query_context_added_events(
        &self,
        params: ParamsOfQueryProfileEvents,
    ) -> KitResult<Vec<ContextAddedEventRecord>> {
        let events = self.query_profile_events(params).await?;
        let mut result = Vec::with_capacity(events.len());

        for event in events {
            let DecodedAuthProfileEvent::ContextAdded { event, data } = event;
            result.push(ContextAddedEventRecord { event, data });
        }

        Ok(result)
    }

    /// # Decode `addContext` cell payload into plain text
    pub fn decode_context_text_cell(&self, context_cell_boc: impl AsRef<str>) -> KitResult<String> {
        let decoded = tvm_client::abi::decode_boc(
            self.context().clone(),
            ParamsOfDecodeBoc {
                params: vec![AbiParam {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    ..Default::default()
                }],
                boc: context_cell_boc.as_ref().to_string(),
                allow_partial: false,
            },
        )
        .map_err(|e| {
            KitError::new(
                KitModule::AuthService(AuthServiceModule::Profile),
                KitErrorCode::Decode,
                "Decode addContext cell into text",
            )
            .with_tvm_error(e)
        })?;

        decoded.data.get("value").and_then(|v| v.as_str()).map(str::to_string).ok_or_else(|| {
            KitError::new(
                KitModule::AuthService(AuthServiceModule::Profile),
                KitErrorCode::Parse,
                "Extract `value` string from decoded addContext cell",
            )
        })
    }
}

fn profile_internal_to_external_address(address: &str) -> String {
    format!(
        ":{}",
        address
            .strip_prefix("0x")
            .or_else(|| address.strip_prefix("0X"))
            .or_else(|| address.strip_prefix("0:"))
            .unwrap_or(address)
    )
}
