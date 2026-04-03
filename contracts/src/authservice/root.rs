use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::ParamsOfDecodeMessageBody;
use tvm_client::abi::Signer;
use tvm_client::net;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::authservice::events::AuthProfileDeployedData;
use crate::authservice::events::AuthServiceEvent;
use crate::authservice::profile::AuthProfile;
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

const ABI: &str = include_str!("../../abi/authservice/AuthServiceRoot.abi.json");

/// Reference wrapper migrated to the reduced-boilerplate style:
/// - stores shared runtime state in `ContractBase`
/// - exposes it via `HasContractBase`
/// - keeps contract identity in `ModuleAccessor`
/// - opts into blanket message/executor impls via `AutoContract`
#[derive(Debug, Clone)]
pub struct AuthServiceRoot {
    base: ContractBase,
}

impl ModuleAccessor for AuthServiceRoot {
    const MODULE: KitModule = KitModule::AuthService(AuthServiceModule::Root);
}

impl HasContractBase for AuthServiceRoot {
    fn base(&self) -> &ContractBase {
        &self.base
    }
}

impl AutoContract for AuthServiceRoot {}

impl AsyncGuarded<Account> for AuthServiceRoot {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T,
    {
        let guard = self.account().lock().await;
        action(&guard)
    }
}

impl AsyncGuardedMut<Account> for AuthServiceRoot {
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
pub struct ParamsOfSetProfileCode {
    pub code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeployProfile {
    #[serde(rename(serialize = "pubkeyHash"))]
    pub pubkey_hash: String,
    #[serde(rename(serialize = "multifactorHash"))]
    pub multifactor_hash: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfDeployProfileByIdentity {
    /// Owner pubkey in hex form. Both `abcd...` and `0xabcd...` are accepted.
    pub pubkey: String,
    /// Multifactor wallet address.
    pub multifactor_address: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetProfileAddress {
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetProfileAddress {
    pub profile: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfHashPubkey {
    pub pubkey: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfHashPubkey {
    pub hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfHashMultifactor {
    pub multifactor: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfHashMultifactor {
    pub hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfQueryProfilesByMultifactor {
    /// Multifactor wallet address.
    pub multifactor: String,
    /// Lower bound (inclusive) for event message timestamp in seconds.
    pub created_at_from: Option<u64>,
    /// Maximum number of decoded records returned to caller.
    pub limit: Option<u32>,
    /// GraphQL pagination cursor (`events.before`).
    pub before: Option<String>,
}

impl Default for ParamsOfQueryProfilesByMultifactor {
    fn default() -> Self {
        Self { multifactor: String::new(), created_at_from: None, limit: Some(50), before: None }
    }
}

/// Decoded `AuthProfileDeployed` record scoped to a concrete `AuthServiceRoot`.
#[derive(Debug, Clone)]
pub struct AuthProfileDeployedEventRecord {
    pub event: crate::event::Event,
    pub data: AuthProfileDeployedData,
}

/// Result of `query_profiles_by_multifactor` with relay cursor pagination state.
#[derive(Debug, Clone)]
pub struct QueryProfilesByMultifactorResult {
    pub records: Vec<AuthProfileDeployedEventRecord>,
    /// Relay cursor of the oldest edge — pass as `before` to fetch the next (older) page.
    pub oldest_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfUpdateCode {
    #[serde(rename(serialize = "newcode"))]
    pub new_code: String,
    pub cell: String,
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
    cursor: String,
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

fn oldest_edge_cursor(edges: &[GqlEdge]) -> Option<String> {
    edges.first().map(|edge| edge.cursor.clone())
}

const GQL_AUTHSERVICE_ROOT_EVENTS_QUERY: &str = r#"
    query($address: String!, $dst: String!, $last: Int!, $before: String) {
      blockchain {
        account(address: $address) {
          events(dst: $dst, last: $last, before: $before) {
            edges {
              cursor
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

impl AuthServiceRoot {
    pub const DEFAULT_ADDRESS: &'static str =
        "0:0404040404040404040404040404040404040404040404040404040404040404";

    /// Creates AuthServiceRoot wrapper bound to the default address.
    pub fn new(context: Arc<ClientContext>) -> Self {
        Self { base: ContractBase::new(context, Self::DEFAULT_ADDRESS, Abi::Json(ABI.to_string())) }
    }

    /// # Set auth profile code
    ///
    /// Original contract method: `setProfileCode`
    ///
    /// Should be signed with root keys
    pub async fn set_profile_code(
        &self,
        params: ParamsOfSetProfileCode,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setProfileCode".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy auth profile
    ///
    /// Original contract method: `deployProfile`
    ///
    /// Open method, can be called by any external sender
    pub async fn deploy_profile(
        &self,
        params: ParamsOfDeployProfile,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployProfile".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy auth profile (developer-friendly)
    ///
    /// Convenience wrapper over `deployProfile` that accepts raw owner pubkey and
    /// multifactor address, hashes them with on-chain getters, and submits deploy.
    pub async fn deploy_profile_by_identity(
        &self,
        params: ParamsOfDeployProfileByIdentity,
        signer: Signer,
    ) -> KitResult<ResultOfSendMessage> {
        let pubkey = if params.pubkey.starts_with("0x") || params.pubkey.starts_with("0X") {
            params.pubkey
        } else {
            format!("0x{}", params.pubkey)
        };

        let pubkey_hash = self.hash_pubkey(ParamsOfHashPubkey { pubkey }).await?.hash;
        let multifactor_hash = self
            .hash_multifactor(ParamsOfHashMultifactor { multifactor: params.multifactor_address })
            .await?
            .hash;

        self.deploy_profile(
            ParamsOfDeployProfile {
                pubkey_hash,
                multifactor_hash,
                description: params.description,
            },
            signer,
        )
        .await
    }

    /// # Get auth profile address
    ///
    /// Original contract method: `getProfileAddress`
    pub async fn get_profile_address(
        &self,
        params: ParamsOfGetProfileAddress,
    ) -> KitResult<ResultOfGetProfileAddress> {
        self.call_get_method_with::<ResultOfGetProfileAddress, ParamsOfGetProfileAddress>(
            "getProfileAddress",
            params,
        )
        .await
    }

    /// # Get auth profile instance
    ///
    /// Original contract method: `getProfileAddress`
    pub async fn get_profile(&self, params: ParamsOfGetProfileAddress) -> KitResult<AuthProfile> {
        let profile = self.get_profile_address(params).await?;
        Ok(AuthProfile::new(self.context().clone(), profile.profile))
    }

    /// # Hash pubkey
    ///
    /// Original contract method: `hashPubkey`
    pub async fn hash_pubkey(&self, params: ParamsOfHashPubkey) -> KitResult<ResultOfHashPubkey> {
        self.call_get_method_with::<ResultOfHashPubkey, ParamsOfHashPubkey>("hashPubkey", params)
            .await
    }

    /// # Hash multifactor address
    ///
    /// Original contract method: `hashMultifactor`
    pub async fn hash_multifactor(
        &self,
        params: ParamsOfHashMultifactor,
    ) -> KitResult<ResultOfHashMultifactor> {
        self.call_get_method_with::<ResultOfHashMultifactor, ParamsOfHashMultifactor>(
            "hashMultifactor",
            params,
        )
        .await
    }

    /// # Query profiles by multifactor wallet (scoped to this root)
    ///
    /// Developer-friendly helper that:
    /// 1. Calculates `multifactorHash` using `hashMultifactor(multifactor)`.
    /// 2. Queries root events sent to `extern(multifactorHash)`.
    /// 3. Decodes `AuthProfileDeployed` events and returns matched profiles.
    ///
    /// This searches only events emitted by the current `AuthServiceRoot`
    /// instance (`src == self.address()`).
    pub async fn query_profiles_by_multifactor(
        &self,
        params: ParamsOfQueryProfilesByMultifactor,
    ) -> KitResult<QueryProfilesByMultifactorResult> {
        let multifactor_hash = self
            .hash_multifactor(ParamsOfHashMultifactor { multifactor: params.multifactor })
            .await?
            .hash;
        let expected_dst =
            AuthServiceEvent::auth_profile_deployed_external_address(&multifactor_hash)?;
        let limit = params.limit.unwrap_or(50);

        let raw = net::query(
            self.context().clone(),
            net::ParamsOfQuery {
                query: GQL_AUTHSERVICE_ROOT_EVENTS_QUERY.to_string(),
                variables: Some(json!({
                    "address": self.address(),
                    "dst": expected_dst,
                    "last": limit,
                    "before": params.before,
                })),
            },
        )
        .await
        .map_err(|e| {
            KitError::new(
                KitModule::AuthService(AuthServiceModule::Root),
                KitErrorCode::QueryEvents,
                "Query AuthServiceRoot events with GraphQL",
            )
            .with_tvm_error(e)
        })?;

        let parsed: GqlMessagesResponse = serde_json::from_value(raw.result).map_err(|e| {
            KitError::new(
                KitModule::AuthService(AuthServiceModule::Root),
                KitErrorCode::DeserializeFailed,
                format!("Deserialize AuthServiceRoot GraphQL response ({e})"),
            )
        })?;

        let created_at_from = params.created_at_from.unwrap_or_default();
        let oldest_cursor = oldest_edge_cursor(&parsed.data.blockchain.account.events.edges);
        let mut result = Vec::new();
        for edge in parsed.data.blockchain.account.events.edges.into_iter() {
            let event_node = edge.node;
            if event_node.created_at < created_at_from {
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
                    KitModule::AuthService(AuthServiceModule::Root),
                    KitErrorCode::Decode,
                    "Decode AuthServiceRoot event body",
                )
                .with_tvm_error(e)
            })?;

            if decoded.name != "AuthProfileDeployed" {
                continue;
            }

            let raw_value = decoded.value.ok_or_else(|| {
                KitError::new(
                    KitModule::AuthService(AuthServiceModule::Root),
                    KitErrorCode::EmptyData,
                    "Decoded AuthProfileDeployed event body has empty data",
                )
            })?;

            let data =
                serde_json::from_value::<AuthProfileDeployedData>(raw_value).map_err(|e| {
                    KitError::new(
                        KitModule::AuthService(AuthServiceModule::Root),
                        KitErrorCode::DeserializeFailed,
                        format!("Deserialize AuthProfileDeployed event body ({e})"),
                    )
                })?;

            let event = crate::event::Event {
                id: event_node.msg_id,
                dst: event_node.dst,
                created_at: event_node.created_at,
                body: event_node.body,
            };
            result.push(AuthProfileDeployedEventRecord { event, data });
            if result.len() >= limit as usize {
                break;
            }
        }

        Ok(QueryProfilesByMultifactorResult { records: result, oldest_cursor })
    }

    /// # Update root code
    ///
    /// Original contract method: `updateCode`
    ///
    /// Should be signed with root keys
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
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use num_bigint::BigUint;
    use tvm_client::abi::Signer;
    use tvm_client::crypto;
    use tvm_client::crypto::KeyPair;
    use tvm_client::crypto::ParamsOfMnemonicDeriveSignKeys;
    use tvm_client::crypto::ParamsOfMnemonicFromRandom;

    use super::*;
    use crate::account::AccountStatus;
    use crate::account::ParamsOfWaitAccount;
    use crate::authservice::profile::AuthProfile;
    use crate::authservice::profile::ParamsOfQueryProfileEvents;
    use crate::error::KitError;
    use crate::giver::top_up_native_with_giver_if_below;
    use crate::mvsystem::multifactor::AccountData as MultifactorAccountData;
    use crate::mvsystem::multifactor::Multifactor;
    use crate::mvsystem::multifactor::ParamsOfGetEpkExpire;
    use crate::mvsystem::multifactor::ParamsOfSubmitTransaction;
    use crate::tests::create_context;
    use crate::traits::AccountAccessor;
    use crate::traits::AddressAccessor;
    use crate::traits::DecodeAccountData;
    use crate::traits::VersionAccessor;

    const AUTH_SERVICE_ROOT_ADDRESS: &str = AuthServiceRoot::DEFAULT_ADDRESS;
    const AUTH_SERVICE_MULTIFACTOR_ADDRESS: &str = crate::tests::MULTIFACTOR_ADDRESS;
    const AUTH_SERVICE_MULTIFACTOR_EPK: &str = crate::tests::MULTIFACTOR_EPK;
    const AUTH_SERVICE_MULTIFACTOR_ESK: &str = crate::tests::MULTIFACTOR_ESK;
    const AUTH_SERVICE_MULTIFACTOR_EPK_EXPIRE_AT: u64 = crate::tests::MULTIFACTOR_EPK_EXPIRE_AT;

    fn gen_signer_keys(
        context: std::sync::Arc<tvm_client::ClientContext>,
        word_count: u8,
    ) -> Result<KeyPair, tvm_client::error::ClientError> {
        let phrase = crypto::mnemonic_from_random(
            context.clone(),
            ParamsOfMnemonicFromRandom { dictionary: None, word_count: Some(word_count) },
        )?
        .phrase;

        crypto::mnemonic_derive_sign_keys(
            context,
            ParamsOfMnemonicDeriveSignKeys {
                phrase,
                path: None,
                dictionary: None,
                word_count: Some(word_count),
            },
        )
    }

    fn parse_u256_str(value: &str) -> BigUint {
        if let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
            return BigUint::parse_bytes(hex.as_bytes(), 16).expect("valid hex uint256");
        }
        BigUint::parse_bytes(value.as_bytes(), 10).expect("valid decimal uint256")
    }

    fn multifactor_epk_signer() -> Signer {
        Signer::Keys {
            keys: KeyPair {
                public: AUTH_SERVICE_MULTIFACTOR_EPK.to_string(),
                secret: AUTH_SERVICE_MULTIFACTOR_ESK.to_string(),
            },
        }
    }

    async fn destroy_profile_via_multifactor(
        context: std::sync::Arc<tvm_client::ClientContext>,
        profile: &AuthProfile,
    ) -> bool {
        let multifactor = Multifactor::new(context.clone(), AUTH_SERVICE_MULTIFACTOR_ADDRESS);
        top_up_native_with_giver_if_below(
            context.clone(),
            &multifactor,
            3_000_000_000,
            5_000_000_000,
            "AuthServiceMultifactor",
        )
        .await;

        let chain_epk_expire_at = multifactor
            .get_epk_expire_at(ParamsOfGetEpkExpire {
                epk: AUTH_SERVICE_MULTIFACTOR_EPK.to_string(),
            })
            .await
            .map(|v| v.epk_expire_at)
            .ok();
        let chain_epks =
            multifactor.get_zkp_ephemeral_public_keys().await.map(|v| v.keys).unwrap_or_default();
        let multifactor_data: Option<MultifactorAccountData> =
            if multifactor.fetch_account().await.is_ok() {
                let raw_data = {
                    let guard = multifactor.account().lock().await;
                    guard.data.clone()
                };
                raw_data.as_ref().and_then(|data| multifactor.decode_account_data(data).ok())
            } else {
                None
            };
        if let Some(data) = multifactor_data.as_ref() {
            eprintln!(
                "multifactor diagnostics: owner_pubkey={}, factors_len={}, epk_count={}, chain_epk_expire_at={:?}",
                data.owner_pubkey,
                data.factors_len,
                chain_epks.len(),
                chain_epk_expire_at
            );
        } else {
            eprintln!(
                "multifactor diagnostics: unable to decode account data, epk_count={}, chain_epk_expire_at={:?}",
                chain_epks.len(),
                chain_epk_expire_at,
            );
        }

        let epk_expire_at = chain_epk_expire_at
            .filter(|v| *v > 0)
            .unwrap_or(AUTH_SERVICE_MULTIFACTOR_EPK_EXPIRE_AT);

        let max_attempts = 4;
        for attempt in 1..=max_attempts {
            let result = multifactor
                .submit_transaction(
                    ParamsOfSubmitTransaction {
                        dest: profile.address().to_string(),
                        value: 1_000_000_000,
                        cc: HashMap::new(),
                        bounce: false,
                        all_balance: false,
                        epk_expire_at,
                        payload: String::new(),
                    },
                    multifactor_epk_signer(),
                )
                .await;

            match result {
                Ok(_) => return true,
                Err(err) => {
                    if let Some(exit_code) = extract_tvm_exit_code(&err) {
                        if exit_code == 501 {
                            eprintln!(
                                "multifactor submit_transaction destroy rejected with exit_code=501; skip cleanup for profile {}",
                                profile.address()
                            );
                            return false;
                        }
                    }

                    if is_transient_network_error(&err) && attempt < max_attempts {
                        eprintln!(
                            "multifactor submit_transaction destroy transient network error on attempt {attempt}/{max_attempts}: {err:?}"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                        continue;
                    }

                    panic!("Destroy profile through multifactor internal transfer failed: {err:?}");
                }
            }
        }

        false
    }

    fn extract_tvm_exit_code(err: &KitError) -> Option<i64> {
        let value =
            err.tvm_error.as_ref()?.data.pointer("/node_error/extensions/details/exit_code")?;
        value.as_i64().or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok()))
    }

    fn is_transient_network_error(err: &KitError) -> bool {
        let Some(tvm_error) = err.tvm_error.as_ref() else {
            return false;
        };

        let msg = tvm_error.message.to_ascii_lowercase();
        msg.contains("connection reset by peer")
            || msg.contains("client error (sendrequest)")
            || msg.contains("all attempts failed")
    }

    async fn expect_unauthorized_exit_code_101_with_retry<F, Fut>(label: &str, mut action: F)
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = KitResult<ResultOfSendMessage>>,
    {
        let max_attempts = 4;

        for attempt in 1..=max_attempts {
            match action().await {
                Ok(_) => panic!("{label} unexpectedly succeeded"),
                Err(err) => {
                    if let Some(exit_code) = extract_tvm_exit_code(&err) {
                        assert_eq!(
                            exit_code, 101,
                            "{label} failed with unexpected TVM exit code `{exit_code}`: {err:?}"
                        );
                        eprintln!("{label} rejected with expected exit_code=101");
                        return;
                    }

                    if is_transient_network_error(&err) && attempt < max_attempts {
                        eprintln!(
                            "{label} transient network error on attempt {attempt}/{max_attempts}: {err:?}"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                        continue;
                    }

                    panic!(
                        "{label} failed without TVM exit_code=101 (non-contract or unretryable error): {err:?}"
                    );
                }
            }
        }

        unreachable!("loop must return or panic");
    }

    async fn expect_success_with_retry<F, Fut, T>(label: &str, mut action: F) -> T
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = KitResult<T>>,
    {
        let max_attempts = 4;
        for attempt in 1..=max_attempts {
            match action().await {
                Ok(value) => return value,
                Err(err) => {
                    if is_transient_network_error(&err) && attempt < max_attempts {
                        eprintln!(
                            "{label} transient network error on attempt {attempt}/{max_attempts}: {err:?}"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                        continue;
                    }

                    panic!("{label} failed: {err:?}");
                }
            }
        }

        unreachable!("loop must return or panic");
    }

    async fn wait_profile_found_by_multifactor(
        root: &AuthServiceRoot,
        multifactor_address: impl AsRef<str>,
        created_at_from: u64,
        expected_profile: impl AsRef<str>,
    ) -> Option<AuthProfileDeployedEventRecord> {
        let multifactor_address = multifactor_address.as_ref().to_string();
        let expected_profile = expected_profile.as_ref().to_string();

        for _ in 0..10 {
            let query_result = match root
                .query_profiles_by_multifactor(ParamsOfQueryProfilesByMultifactor {
                    multifactor: multifactor_address.clone(),
                    created_at_from: Some(created_at_from),
                    limit: Some(50),
                    before: None,
                })
                .await
            {
                Ok(records) => records,
                Err(err) => {
                    eprintln!("query_profiles_by_multifactor failed: {err:?}");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
            };
            eprintln!(
                "query_profiles_by_multifactor fetched {} decoded events (oldest_cursor={:?})",
                query_result.records.len(),
                query_result.oldest_cursor
            );

            for record in query_result.records {
                if record.data.profile.eq_ignore_ascii_case(&expected_profile) {
                    return Some(record);
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        None
    }

    #[tokio::test]
    async fn test_deploy_profile_flow() {
        let context = create_context();
        let root = AuthServiceRoot::new(context.clone());
        top_up_native_with_giver_if_below(
            context.clone(),
            &root,
            3_000_000_000,
            5_000_000_000,
            "AuthServiceRoot",
        )
        .await;

        let owner_keys = gen_signer_keys(context.clone(), 24).expect("Generate owner keys");
        let stranger_keys = gen_signer_keys(context.clone(), 24).expect("Generate stranger keys");
        let owner_signer = Signer::Keys { keys: owner_keys.clone() };
        let stranger_signer = Signer::Keys { keys: stranger_keys.clone() };

        let started_at =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_secs().saturating_sub(5);

        let description = format!("ackinacki-kit authservice deploy test {}", started_at);
        let owner_pubkey_hex = owner_keys.public.clone();
        let owner_pubkey_with_prefix = format!("0x{owner_pubkey_hex}");
        let multifactor_address = AUTH_SERVICE_MULTIFACTOR_ADDRESS.to_string();

        let root_version = root
            .get_version()
            .await
            .inspect_err(|e| eprintln!("root getVersion failed: {e:?}"))
            .expect("Read root version");
        assert_eq!(root_version.contract_name, "AuthServiceRoot");

        let pubkey_hash = root
            .hash_pubkey(ParamsOfHashPubkey { pubkey: owner_pubkey_with_prefix })
            .await
            .inspect_err(|e| eprintln!("hashPubkey failed: {e:?}"))
            .expect("Hash pubkey")
            .hash;
        let multifactor_hash = root
            .hash_multifactor(ParamsOfHashMultifactor { multifactor: multifactor_address.clone() })
            .await
            .inspect_err(|e| eprintln!("hashMultifactor failed: {e:?}"))
            .expect("Hash multifactor")
            .hash;

        let expected_profile = root
            .get_profile_address(ParamsOfGetProfileAddress { description: description.clone() })
            .await
            .inspect_err(|e| eprintln!("getProfileAddress failed: {e:?}"))
            .expect("Get deterministic profile address")
            .profile;

        expect_success_with_retry("deployProfile", || {
            root.deploy_profile(
                ParamsOfDeployProfile {
                    pubkey_hash: pubkey_hash.clone(),
                    multifactor_hash: multifactor_hash.clone(),
                    description: description.clone(),
                },
                Signer::None,
            )
        })
        .await;

        let profile = root
            .get_profile(ParamsOfGetProfileAddress { description: description.clone() })
            .await
            .inspect_err(|e| eprintln!("getProfile failed: {e:?}"))
            .expect("Get profile wrapper");
        eprintln!("Deployed profile address: {}", profile.address());

        profile
            .wait_account(ParamsOfWaitAccount {
                status: AccountStatus::Active,
                attempts: Some(30),
                attempts_timeout: Some(2_000),
            })
            .await
            .inspect_err(|e| eprintln!("wait profile failed: {e:?}"))
            .expect("Wait profile active");

        let details = profile
            .get_details()
            .await
            .inspect_err(|e| eprintln!("getDetails failed: {e:?}"))
            .expect("Read profile details");

        assert_eq!(details.description, description);
        assert_eq!(details.root.to_lowercase(), AUTH_SERVICE_ROOT_ADDRESS.to_lowercase());
        assert_ne!(parse_u256_str(&details.description_hash), BigUint::default());
        assert_eq!(parse_u256_str(&details.pubkey_hash), parse_u256_str(&pubkey_hash));
        assert_eq!(parse_u256_str(&details.multifactor_hash), parse_u256_str(&multifactor_hash));
        assert_eq!(profile.address().to_lowercase(), expected_profile.to_lowercase());

        let profile_version = profile
            .get_version()
            .await
            .inspect_err(|e| eprintln!("profile getVersion failed: {e:?}"))
            .expect("Read profile version");
        assert_eq!(profile_version.contract_name, "AuthProfile");

        let context_text = "ackinacki-kit authservice context".to_string();

        expect_unauthorized_exit_code_101_with_retry("stranger addContext", || {
            profile.add_context_text(&context_text, stranger_signer.clone())
        })
        .await;
        profile
            .wait_account(ParamsOfWaitAccount {
                status: AccountStatus::Active,
                attempts: Some(10),
                attempts_timeout: Some(1_000),
            })
            .await
            .inspect_err(|e| eprintln!("wait after stranger addContext failed: {e:?}"))
            .expect("Profile remains active after stranger addContext");

        expect_success_with_retry("owner addContext", || {
            profile.add_context_text(context_text.as_str(), owner_signer.clone())
        })
        .await;

        let found_profile_event: Option<AuthProfileDeployedEventRecord> =
            wait_profile_found_by_multifactor(
                &root,
                &multifactor_address,
                started_at,
                &expected_profile,
            )
            .await;
        let found_profile_event = found_profile_event.unwrap_or_else(|| {
            panic!(
                "AuthProfileDeployed event for profile `{expected_profile}` was not found via query_profiles_by_multifactor"
            )
        });
        assert_eq!(
            parse_u256_str(&found_profile_event.data.multifactor_hash),
            parse_u256_str(&multifactor_hash)
        );
        assert_eq!(found_profile_event.data.description, description);
        eprintln!(
            "AuthProfileDeployed (query_profiles_by_multifactor) raw event: {:?}",
            found_profile_event.event
        );
        eprintln!(
            "AuthProfileDeployed (query_profiles_by_multifactor) decoded data: {:?}",
            found_profile_event.data
        );
        let destroyed = destroy_profile_via_multifactor(context.clone(), &profile).await;
        if destroyed {
            profile
                .wait_account(ParamsOfWaitAccount {
                    status: AccountStatus::NonExist,
                    attempts: Some(30),
                    attempts_timeout: Some(2_000),
                })
                .await
                .inspect_err(|e| eprintln!("wait profile destroyed failed: {e:?}"))
                .expect("Wait profile destroyed");
            eprintln!("Destroyed profile address: {}", profile.address());
        } else {
            eprintln!("Skipped profile cleanup: multifactor rejected destroy");
        }
    }

    #[tokio::test]
    async fn test_add_context_message_found() {
        let context = create_context();
        let root = AuthServiceRoot::new(context.clone());
        top_up_native_with_giver_if_below(
            context.clone(),
            &root,
            3_000_000_000,
            5_000_000_000,
            "AuthServiceRoot",
        )
        .await;

        let owner_keys = gen_signer_keys(context.clone(), 24).expect("Generate owner keys");
        let owner_signer = Signer::Keys { keys: owner_keys.clone() };
        let owner_pubkey_with_prefix = format!("0x{}", owner_keys.public);
        let multifactor_address = AUTH_SERVICE_MULTIFACTOR_ADDRESS.to_string();

        let started_at =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_secs().saturating_sub(5);

        let description = format!("ackinacki-kit authservice context msg test {}", started_at);
        let pubkey_hash = root
            .hash_pubkey(ParamsOfHashPubkey { pubkey: owner_pubkey_with_prefix })
            .await
            .inspect_err(|e| eprintln!("hashPubkey failed: {e:?}"))
            .expect("Hash pubkey")
            .hash;
        let multifactor_hash = root
            .hash_multifactor(ParamsOfHashMultifactor { multifactor: multifactor_address.clone() })
            .await
            .inspect_err(|e| eprintln!("hashMultifactor failed: {e:?}"))
            .expect("Hash multifactor")
            .hash;

        let expected_profile = root
            .get_profile_address(ParamsOfGetProfileAddress { description: description.clone() })
            .await
            .inspect_err(|e| eprintln!("getProfileAddress failed: {e:?}"))
            .expect("Get deterministic profile address")
            .profile;

        expect_success_with_retry("deployProfile", || {
            root.deploy_profile(
                ParamsOfDeployProfile {
                    pubkey_hash: pubkey_hash.clone(),
                    multifactor_hash: multifactor_hash.clone(),
                    description: description.clone(),
                },
                Signer::None,
            )
        })
        .await;

        let profile = root
            .get_profile(ParamsOfGetProfileAddress { description: description.clone() })
            .await
            .inspect_err(|e| eprintln!("getProfile failed: {e:?}"))
            .expect("Get profile wrapper");
        assert_eq!(profile.address().to_lowercase(), expected_profile.to_lowercase());
        eprintln!("Deployed profile for ContextAdded test: {}", profile.address());

        profile
            .wait_account(ParamsOfWaitAccount {
                status: AccountStatus::Active,
                attempts: Some(30),
                attempts_timeout: Some(2_000),
            })
            .await
            .inspect_err(|e| eprintln!("wait profile active failed: {e:?}"))
            .expect("Wait profile active");

        let found_profile_event: Option<AuthProfileDeployedEventRecord> =
            wait_profile_found_by_multifactor(
                &root,
                &multifactor_address,
                started_at,
                profile.address(),
            )
            .await;
        let found_profile_event = found_profile_event.unwrap_or_else(|| {
            panic!(
                "AuthProfileDeployed event for profile `{}` was not found via query_profiles_by_multifactor",
                profile.address()
            )
        });
        assert_eq!(
            found_profile_event.data.profile.to_lowercase(),
            profile.address().to_lowercase()
        );
        assert_eq!(
            parse_u256_str(&found_profile_event.data.multifactor_hash),
            parse_u256_str(&multifactor_hash)
        );
        assert_eq!(found_profile_event.data.description, description);

        let context_text = "ackinacki-kit ContextAdded payload check".to_string();
        let context_cell = profile
            .encode_context_text_cell(&context_text)
            .inspect_err(|e| eprintln!("encode_context_text_cell failed: {e:?}"))
            .expect("Encode context text into cell");
        let add_context_started_at =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_secs().saturating_sub(2);

        expect_success_with_retry("owner addContext", || {
            profile.add_context_text(context_text.as_str(), owner_signer.clone())
        })
        .await;

        let mut found_context_added = false;

        for _ in 0..10 {
            let events = match profile
                .query_context_added_events(ParamsOfQueryProfileEvents {
                    created_at_from: Some(add_context_started_at),
                    limit: Some(50),
                    before: None,
                })
                .await
            {
                Ok(result) => result.events,
                Err(err) => {
                    if is_transient_network_error(&err) {
                        eprintln!("query_context_added_events transient network error: {err:?}");
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                    panic!("query_context_added_events failed: {err:?}");
                }
            };
            eprintln!("query_context_added_events fetched {} decoded events", events.len());

            for decoded_event in events {
                let event = decoded_event.event;
                let data = decoded_event.data;
                assert_eq!(data.text, context_text);
                assert_eq!(profile.decode_context_text_cell(&context_cell).unwrap(), context_text);
                eprintln!("ContextAdded raw event: {:?}", event);
                eprintln!("ContextAdded decoded event: {:?}", data);
                found_context_added = true;
                break;
            }

            if found_context_added {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        assert!(
            found_context_added,
            "ContextAdded message was not found for profile `{}`",
            profile.address(),
        );

        let destroyed = destroy_profile_via_multifactor(context.clone(), &profile).await;
        if destroyed {
            profile
                .wait_account(ParamsOfWaitAccount {
                    status: AccountStatus::NonExist,
                    attempts: Some(30),
                    attempts_timeout: Some(2_000),
                })
                .await
                .inspect_err(|e| eprintln!("wait profile destroyed failed: {e:?}"))
                .expect("Wait profile destroyed");
            eprintln!("Destroyed profile address: {}", profile.address());
        } else {
            eprintln!("Skipped profile cleanup: multifactor rejected destroy");
        }
    }

    #[test]
    fn test_query_profiles_gql_includes_relay_cursor() {
        assert!(GQL_AUTHSERVICE_ROOT_EVENTS_QUERY
            .contains("events(dst: $dst, last: $last, before: $before)"));
        assert!(GQL_AUTHSERVICE_ROOT_EVENTS_QUERY.contains("edges"));
        assert!(GQL_AUTHSERVICE_ROOT_EVENTS_QUERY.contains("cursor"));
    }

    #[test]
    fn test_oldest_edge_cursor_is_extracted_from_gql_response() {
        let raw = serde_json::json!({
            "data": {
                "blockchain": {
                    "account": {
                        "events": {
                            "edges": [
                                {
                                    "cursor": "cursor_1",
                                    "node": {
                                        "msg_id": "msg_1",
                                        "created_at": 1u64,
                                        "dst": ":1",
                                        "body": "te6ccgEBAQEAAgAAAA=="
                                    }
                                },
                                {
                                    "cursor": "cursor_2",
                                    "node": {
                                        "msg_id": "msg_2",
                                        "created_at": 2u64,
                                        "dst": ":2",
                                        "body": "te6ccgEBAQEAAgAAAA=="
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        });

        let parsed: GqlMessagesResponse =
            serde_json::from_value(raw).expect("Deserialize GraphQL response with edges.cursor");
        let cursor = oldest_edge_cursor(&parsed.data.blockchain.account.events.edges);
        assert_eq!(cursor.as_deref(), Some("cursor_1"));
    }

    #[test]
    fn test_oldest_edge_cursor_is_none_for_empty_edges() {
        assert_eq!(oldest_edge_cursor(&[]), None);
    }
}
