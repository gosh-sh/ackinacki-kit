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
use crate::authservice::profile::AuthProfile;
use crate::error::AuthServiceModule;
use crate::error::KitModule;
use crate::traits::AutoContract;
use crate::traits::AccountAccessor;
use crate::traits::ContractBase;
use crate::traits::ContextAccessor;
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
pub struct ParamsOfUpdateCode {
    #[serde(rename(serialize = "newcode"))]
    pub new_code: String,
    pub cell: String,
}

impl AuthServiceRoot {
    pub const DEFAULT_ADDRESS: &'static str =
        "0:0404040404040404040404040404040404040404040404040404040404040404";

    /// Allows passing the root address explicitly (useful for shellnet/testnet
    /// or local networks where AuthServiceRoot may live at a non-premine address).
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            base: ContractBase::new(context, address, Abi::Json(ABI.to_string())),
        }
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
    use std::future::Future;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use num_bigint::BigUint;
    use serde_json::json;
    use tvm_client::abi::Signer;
    use tvm_client::crypto;
    use tvm_client::crypto::KeyPair;
    use tvm_client::crypto::ParamsOfMnemonicDeriveSignKeys;
    use tvm_client::crypto::ParamsOfMnemonicFromRandom;

    use crate::account::AccountStatus;
    use crate::account::ParamsOfWaitAccount;
    use crate::authservice::events::AuthServiceEvent;
    use crate::authservice::events::DecodedAuthServiceEvent;
    use crate::error::KitError;
    use crate::authservice::profile::ParamsOfQueryProfileEvents;
    use crate::event::query_events;
    use crate::tests::create_context;
    use crate::tests::top_up_native_with_giver_if_below;
    use crate::traits::AccountAccessor;
    use crate::traits::AddressAccessor;
    use crate::traits::FromEvent;
    use crate::traits::VersionAccessor;

    const SHELLNET_AUTH_SERVICE_ROOT_ADDRESS: &str =
        "0:df9a74d0ec1977a7b74863e1a468f1e2de1962ab503bc67f15b6a96298488224";

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

    fn extract_tvm_exit_code(err: &KitError) -> Option<i64> {
        let value = err
            .tvm_error
            .as_ref()?
            .data
            .pointer("/node_error/extensions/details/exit_code")?;
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

    #[tokio::test]
    async fn test_deploy_profile_on_shellnet() {
        let context = create_context();
        let root = AuthServiceRoot::new(context.clone(), SHELLNET_AUTH_SERVICE_ROOT_ADDRESS);
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

        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs()
            .saturating_sub(5);

        let description = format!("ackinacki-kit authservice deploy test {}", started_at);
        let owner_pubkey_hex = owner_keys.public.clone();
        let owner_pubkey_with_prefix = format!("0x{owner_pubkey_hex}");

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

        let expected_profile = root
            .get_profile_address(ParamsOfGetProfileAddress { description: description.clone() })
            .await
            .inspect_err(|e| eprintln!("getProfileAddress failed: {e:?}"))
            .expect("Get deterministic profile address")
            .profile;

        root.deploy_profile(
            ParamsOfDeployProfile { pubkey_hash: pubkey_hash.clone(), description: description.clone() },
            Signer::None,
        )
        .await
        .inspect_err(|e| eprintln!("deployProfile failed: {e:?}"))
        .expect("Deploy profile");

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
        assert_eq!(details.root.to_lowercase(), SHELLNET_AUTH_SERVICE_ROOT_ADDRESS.to_lowercase());
        assert_ne!(parse_u256_str(&details.description_hash), BigUint::default());
        assert_eq!(parse_u256_str(&details.pubkey_hash), parse_u256_str(&pubkey_hash));
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

        profile
            .add_context_text(&context_text, owner_signer.clone())
            .await
            .inspect_err(|e| eprintln!("owner addContext failed: {e:?}"))
            .expect("Owner addContext succeeds");

        let event_address = AuthServiceEvent::auth_profile_deployed_external_address(&pubkey_hash)
            .expect("Build authservice event address");
        let mut matched_event = false;

        for _ in 0..10 {
            let events = query_events(
                context.clone(),
                Some(json!({
                    "src": { "eq": SHELLNET_AUTH_SERVICE_ROOT_ADDRESS },
                    "dst": { "eq": event_address },
                    "created_at": { "ge": started_at },
                })),
                None,
                Some(50),
            )
            .await
            .inspect_err(|e| eprintln!("query_events failed: {e:?}"))
            .expect("Query authservice events");

            for event in events {
                let decoded = DecodedAuthServiceEvent::from_event(&event, &root)
                    .inspect_err(|e| eprintln!("decode authservice event failed: {e:?}"))
                    .expect("Decode authservice event");

                let DecodedAuthServiceEvent::AuthProfileDeployed { data, .. } = decoded;
                if data.profile.eq_ignore_ascii_case(&expected_profile) {
                    matched_event = true;
                    break;
                }
            }

            if matched_event {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        if !matched_event {
            eprintln!(
                "Warning: AuthProfileDeployed event for profile `{expected_profile}` was not observed on shellnet in the polling window"
            );
        }

        expect_unauthorized_exit_code_101_with_retry("stranger destroy", || {
            profile.destroy(stranger_signer.clone())
        })
        .await;
        profile
            .wait_account(ParamsOfWaitAccount {
                status: AccountStatus::Active,
                attempts: Some(10),
                attempts_timeout: Some(1_000),
            })
            .await
            .inspect_err(|e| eprintln!("wait after stranger destroy failed: {e:?}"))
            .expect("Profile remains active after stranger destroy");

        profile
            .destroy(owner_signer)
            .await
            .inspect_err(|e| eprintln!("destroy failed: {e:?}"))
            .expect("Destroy profile");

        profile
            .wait_account(ParamsOfWaitAccount {
                status: AccountStatus::NonExist,
                attempts: Some(30),
                attempts_timeout: Some(2_000),
            })
            .await
            .inspect_err(|e| eprintln!("wait profile destroy failed: {e:?}"))
            .expect("Wait profile destroyed");
        eprintln!("Destroyed profile address: {}", expected_profile);
    }

    #[tokio::test]
    async fn test_add_context_message_found_on_shellnet() {
        let context = create_context();
        let root = AuthServiceRoot::new(context.clone(), SHELLNET_AUTH_SERVICE_ROOT_ADDRESS);
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

        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs()
            .saturating_sub(5);

        let description = format!("ackinacki-kit authservice context msg test {}", started_at);
        let pubkey_hash = root
            .hash_pubkey(ParamsOfHashPubkey { pubkey: owner_pubkey_with_prefix })
            .await
            .inspect_err(|e| eprintln!("hashPubkey failed: {e:?}"))
            .expect("Hash pubkey")
            .hash;

        let expected_profile = root
            .get_profile_address(ParamsOfGetProfileAddress { description: description.clone() })
            .await
            .inspect_err(|e| eprintln!("getProfileAddress failed: {e:?}"))
            .expect("Get deterministic profile address")
            .profile;

        root.deploy_profile(
            ParamsOfDeployProfile { pubkey_hash, description: description.clone() },
            Signer::None,
        )
        .await
        .inspect_err(|e| eprintln!("deployProfile failed: {e:?}"))
        .expect("Deploy profile");

        let profile = root
            .get_profile(ParamsOfGetProfileAddress { description })
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

        let context_text = "ackinacki-kit ContextAdded payload check".to_string();
        let context_cell = profile
            .encode_context_text_cell(&context_text)
            .inspect_err(|e| eprintln!("encode_context_text_cell failed: {e:?}"))
            .expect("Encode context text into cell");
        let add_context_started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs()
            .saturating_sub(2);

        profile
            .add_context_text(&context_text, owner_signer.clone())
            .await
            .inspect_err(|e| eprintln!("owner addContext failed: {e:?}"))
            .expect("Owner addContext succeeds");

        let mut found_context_added = false;

        for _ in 0..10 {
            let events = profile
                .query_context_added_events(ParamsOfQueryProfileEvents {
                    created_at_from: Some(add_context_started_at),
                    limit: Some(50),
                    before: None,
                })
                .await
                .inspect_err(|e| eprintln!("query_context_added_events failed: {e:?}"))
                .expect("Query and decode ContextAdded events");
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

        profile
            .destroy(owner_signer)
            .await
            .inspect_err(|e| eprintln!("destroy failed: {e:?}"))
            .expect("Destroy profile");

        profile
            .wait_account(ParamsOfWaitAccount {
                status: AccountStatus::NonExist,
                attempts: Some(30),
                attempts_timeout: Some(2_000),
            })
            .await
            .inspect_err(|e| eprintln!("wait profile destroy failed: {e:?}"))
            .expect("Wait profile destroyed");
    }
}
