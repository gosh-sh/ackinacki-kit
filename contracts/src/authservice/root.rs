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
use tvm_client::net;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::authservice::events::AuthProfileDeployedData;
use crate::authservice::events::AuthServiceEvent;
use crate::authservice::events::DecodedAuthServiceEvent;
use crate::authservice::profile::AuthProfile;
use crate::error::AuthServiceModule;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::AutoContract;
use crate::traits::ContextAccessor;
use crate::traits::ContractBase;
use crate::traits::FromEvent;
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
    /// Maximum number of messages to fetch per GraphQL query.
    pub limit: Option<u32>,
    /// Reverse-pagination cursor (`before`) for GraphQL account.messages query.
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
    messages: GqlMessages,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlMessages {
    edges: Vec<GqlEdge>,
}

#[derive(Debug, Clone, Deserialize)]
struct GqlEdge {
    node: crate::event::Event,
}

const GQL_AUTHSERVICE_ROOT_EVENTS_QUERY: &str = r#"
    query($address: String!, $counterparties: [String!], $last: Int!, $before: String) {
      blockchain {
        account(address: $address) {
          messages(
            msg_type: [ExtOut]
            counterparties: $counterparties
            last: $last
            before: $before
          ) {
            edges {
              node {
                id
                src
                dst
                created_at
                boc
              }
            }
          }
        }
      }
    }
"#;

fn is_valid_profile_deploy_raw_event(
    event: &crate::event::Event,
    expected_src: &str,
    expected_dst: &str,
    created_at_from: u64,
) -> bool {
    event.src.eq_ignore_ascii_case(expected_src)
        && event.dst.eq_ignore_ascii_case(expected_dst)
        && event.created_at >= created_at_from
}

fn is_valid_profile_deploy_data(
    data: &AuthProfileDeployedData,
    expected_multifactor_hash: &str,
) -> bool {
    data.multifactor_hash.eq_ignore_ascii_case(expected_multifactor_hash)
}

impl AuthServiceRoot {
    pub const DEFAULT_ADDRESS: &'static str =
        "0:0404040404040404040404040404040404040404040404040404040404040404";

    /// Allows passing the root address explicitly (useful for non-default
    /// networks where AuthServiceRoot may live at a non-premine address).
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { base: ContractBase::new(context, address, Abi::Json(ABI.to_string())) }
    }

    pub fn new_default(context: Arc<ClientContext>) -> Self {
        Self::new(context, Self::DEFAULT_ADDRESS)
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
    /// 2. Queries root outbound external messages sent to `extern(multifactorHash)`.
    /// 3. Decodes `AuthProfileDeployed` events and returns matched profiles.
    ///
    /// This searches only events emitted by the current `AuthServiceRoot`
    /// instance (`src == self.address()`).
    pub async fn query_profiles_by_multifactor(
        &self,
        params: ParamsOfQueryProfilesByMultifactor,
    ) -> KitResult<Vec<AuthProfileDeployedEventRecord>> {
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
                    "counterparties": [expected_dst],
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
        let mut result = Vec::new();
        for event in parsed.data.blockchain.account.messages.edges.into_iter().map(|e| e.node) {
            if !is_valid_profile_deploy_raw_event(
                &event,
                self.address(),
                &expected_dst,
                created_at_from,
            ) {
                continue;
            }

            let decoded = DecodedAuthServiceEvent::from_event(&event, self)?;
            let DecodedAuthServiceEvent::AuthProfileDeployed { data, .. } = decoded;
            if !is_valid_profile_deploy_data(&data, &multifactor_hash) {
                continue;
            }
            result.push(AuthProfileDeployedEventRecord { event, data });
        }

        Ok(result)
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
    use crate::mvsystem::multifactor::Multifactor;
    use crate::mvsystem::multifactor::ParamsOfSubmitTransaction;
    use crate::tests::create_context;
    use crate::tests::top_up_native_with_giver_if_below;
    use crate::traits::AccountAccessor;
    use crate::traits::AddressAccessor;
    use crate::traits::VersionAccessor;

    const AUTH_SERVICE_ROOT_ADDRESS: &str =
        "0:d6054a384e148b7dac122acf24ec7f218b44826a8a68bb085f2ba371b59ff6a8";
    const AUTH_SERVICE_MULTIFACTOR_ADDRESS: &str =
        "0:b66e32af6b7a93e980948a8ad2dc9283ea39b8d4d05dd8c7b8689cc72e30ec28";
    const AUTH_SERVICE_MULTIFACTOR_EPK: &str =
        "692fa80bd52af31cf4bc6479e7cc9c115eab6e60783471414fd1da557f7ba1c3";
    const AUTH_SERVICE_MULTIFACTOR_ESK: &str =
        "2728424ad101148253fe43dd69ca2ea155228bb4f4b34e39174bd21bb405e249";
    const AUTH_SERVICE_MULTIFACTOR_EPK_EXPIRE_AT: u64 = 1_787_683_748;

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

    #[test]
    fn test_query_profiles_by_multifactor_raw_post_filter_mixed_dst() {
        let expected_src = "0:d6054a384e148b7dac122acf24ec7f218b44826a8a68bb085f2ba371b59ff6a8";
        let expected_dst = ":f968d0686b7853933f28f98d101a21b625be653045ad93740e4db0033aed7a0c";
        let created_at_from = 1_772_219_000_u64;

        let events = vec![
            crate::event::Event {
                id: "valid".to_string(),
                src: expected_src.to_string(),
                dst: expected_dst.to_string(),
                created_at: created_at_from + 10,
                boc: "ignored".to_string(),
            },
            crate::event::Event {
                id: "wrong-dst".to_string(),
                src: expected_src.to_string(),
                dst: ":deadbeef".to_string(),
                created_at: created_at_from + 10,
                boc: "ignored".to_string(),
            },
            crate::event::Event {
                id: "wrong-src".to_string(),
                src: "0:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                dst: expected_dst.to_string(),
                created_at: created_at_from + 10,
                boc: "ignored".to_string(),
            },
            crate::event::Event {
                id: "too-old".to_string(),
                src: expected_src.to_string(),
                dst: expected_dst.to_string(),
                created_at: created_at_from.saturating_sub(1),
                boc: "ignored".to_string(),
            },
        ];

        let filtered = events
            .iter()
            .filter(|event| {
                is_valid_profile_deploy_raw_event(
                    event,
                    expected_src,
                    expected_dst,
                    created_at_from,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "valid");
    }

    #[test]
    fn test_query_profiles_by_multifactor_post_filter_multifactor_hash_mismatch() {
        let expected_multifactor_hash =
            "0xf968d0686b7853933f28f98d101a21b625be653045ad93740e4db0033aed7a0c";

        let valid_data = AuthProfileDeployedData {
            profile: "0:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_string(),
            multifactor_hash: expected_multifactor_hash.to_string(),
            description: "ok".to_string(),
        };
        let mismatch_data = AuthProfileDeployedData {
            profile: "0:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                .to_string(),
            multifactor_hash: "0x1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
            description: "bad".to_string(),
        };

        assert!(is_valid_profile_deploy_data(&valid_data, expected_multifactor_hash));
        assert!(!is_valid_profile_deploy_data(&mismatch_data, expected_multifactor_hash));
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
    ) {
        let multifactor = Multifactor::new(context.clone(), AUTH_SERVICE_MULTIFACTOR_ADDRESS);
        top_up_native_with_giver_if_below(
            context.clone(),
            &multifactor,
            3_000_000_000,
            5_000_000_000,
            "AuthServiceMultifactor",
        )
        .await;

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
                        epk_expire_at: AUTH_SERVICE_MULTIFACTOR_EPK_EXPIRE_AT,
                        payload: String::new(),
                    },
                    multifactor_epk_signer(),
                )
                .await;

            match result {
                Ok(_) => return,
                Err(err) => {
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
            let records = match root
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
            eprintln!("query_profiles_by_multifactor fetched {} decoded events", records.len());

            for record in records {
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
        let root = AuthServiceRoot::new(context.clone(), AUTH_SERVICE_ROOT_ADDRESS);
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
        destroy_profile_via_multifactor(context.clone(), &profile).await;
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
    }

    #[tokio::test]
    async fn test_add_context_message_found() {
        let context = create_context();
        let root = AuthServiceRoot::new(context.clone(), AUTH_SERVICE_ROOT_ADDRESS);
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
                Ok(events) => events,
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
                assert_eq!(event.src.to_lowercase(), profile.address().to_lowercase());
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

        destroy_profile_via_multifactor(context.clone(), &profile).await;
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
    }
}
