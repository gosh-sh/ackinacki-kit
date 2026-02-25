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
    pub pubkey: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfGetProfileAddress {
    pub pubkey: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetProfileAddress {
    pub profile: String,
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
    use crate::event::query_events;
    use crate::tests::create_context;
    use crate::tests::top_up_native_with_giver_if_below;
    use crate::traits::AccountAccessor;
    use crate::traits::AddressAccessor;
    use crate::traits::FromEvent;

    const SHELLNET_AUTH_SERVICE_ROOT_ADDRESS: &str =
        "0:dc02c3729be17598278617915c999c1599eb81b1ff9c07457b2693ed1d49c98b";

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

    fn hex_u256_to_dec(value: &str) -> String {
        BigUint::parse_bytes(value.as_bytes(), 16)
            .expect("valid hex uint256")
            .to_string()
    }

    fn parse_u256_str(value: &str) -> BigUint {
        if let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
            return BigUint::parse_bytes(hex.as_bytes(), 16).expect("valid hex uint256");
        }
        BigUint::parse_bytes(value.as_bytes(), 10).expect("valid decimal uint256")
    }

    #[tokio::test]
    #[ignore = "requires shellnet access and performs a real deployProfile call"]
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

        let keys = gen_signer_keys(context.clone(), 24).expect("Generate signer keys");
        let signer = Signer::Keys { keys: keys.clone() };

        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs()
            .saturating_sub(5);

        let description = format!("ackinacki-kit authservice deploy test {}", started_at);
        let pubkey_hex = keys.public.clone();
        let pubkey = hex_u256_to_dec(&pubkey_hex);

        let expected_profile = root
            .get_profile_address(ParamsOfGetProfileAddress { pubkey: pubkey.clone() })
            .await
            .inspect_err(|e| eprintln!("getProfileAddress failed: {e:?}"))
            .expect("Get deterministic profile address")
            .profile;

        root.deploy_profile(
            ParamsOfDeployProfile { pubkey: pubkey.clone(), description: description.clone() },
            signer,
        )
        .await
        .inspect_err(|e| eprintln!("deployProfile failed: {e:?}"))
        .expect("Deploy profile");

        let profile = root
            .get_profile(ParamsOfGetProfileAddress { pubkey: pubkey.clone() })
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
        assert_eq!(parse_u256_str(&details.pubkey), parse_u256_str(&pubkey));
        assert_eq!(profile.address().to_lowercase(), expected_profile.to_lowercase());

        let event_address = AuthServiceEvent::AuthProfileDeployed.to_address();
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

        profile
            .destroy(Signer::Keys { keys })
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
}
