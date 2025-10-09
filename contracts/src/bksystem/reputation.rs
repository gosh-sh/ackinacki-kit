use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::account::AccountStatus;
use crate::deserialize::deserialize_u128;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::EncodeMessage;
use crate::traits::Executor;

const ABI: &str = include_str!("../../abi/bksystem/ReputationCoefficientCalculator.abi.json");
const TVC: &[u8] = include_bytes!("../../abi/bksystem/ReputationCoefficientCalculator.tvc");

#[derive(Debug, Clone)]
pub struct ReputationCoefficientCalculator {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

#[async_trait]
impl AccountAccessor for ReputationCoefficientCalculator {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }

    async fn fetch_account(&self) -> anyhow::Result<()> {
        let state = base64::prelude::BASE64_STANDARD.encode(TVC);
        let encoded_account = abi::encode_account(
            self.context.clone(),
            abi::ParamsOfEncodeAccount {
                state_init: state.clone(),
                balance: None,
                last_trans_lt: None,
                last_paid: None,
                boc_cache: None,
            },
        )
        .map_err(|e| anyhow!("Encode account ({e})"))?;

        let created_account = Account {
            context: self.context.clone(),
            address: self.address.clone(),
            boc: Some(encoded_account.account),
            data: None,
            balance: None,
            acc_type: AccountStatus::Active,
            ecc: BTreeMap::new(),
        };

        self.async_guarded_mut(|mut account| async move {
            *account = created_account;
            Ok(())
        })
        .await
    }
}

impl AbiAccessor for ReputationCoefficientCalculator {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for ReputationCoefficientCalculator {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for ReputationCoefficientCalculator {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for ReputationCoefficientCalculator {}

impl Executor for ReputationCoefficientCalculator {}

#[async_trait]
impl AsyncGuarded<Account> for ReputationCoefficientCalculator {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Account) -> T + Send + 'async_trait,
        T: Send + 'async_trait,
    {
        let guard = self.account.lock().await;
        action(&guard)
    }
}

#[async_trait]
impl AsyncGuardedMut<Account> for ReputationCoefficientCalculator {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Account>) -> Fut + Send + 'async_trait,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'async_trait,
        T: Send + 'async_trait,
    {
        let guard = self.account.clone().lock_owned().await;
        action(guard).await
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamsOfCalculate {
    #[serde(rename(serialize = "reptime"))]
    pub reputation_time: u128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfCalculate {
    #[serde(deserialize_with = "deserialize_u128")]
    pub value0: u128,
}

impl ReputationCoefficientCalculator {
    pub fn new(context: Arc<ClientContext>) -> Self {
        let address = "0:0000000000000000000000000000000000000000000000000000000000000000";

        Self {
            context: context.clone(),
            address: address.to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context.clone(), address))),
        }
    }

    pub async fn calculate(&self, params: ParamsOfCalculate) -> anyhow::Result<u128> {
        let call_set = CallSet {
            function_name: "calcRepCoef".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        let calculated = match result.decoded {
            Some(data) => match data.output {
                Some(value) => {
                    let data = serde_json::from_value::<ResultOfCalculate>(value)
                        .map_err(|e| anyhow!("Deserialize output ({})", e))?;
                    data.value0
                }
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        };

        Ok(calculated)
    }
}

#[cfg(test)]
mod tests {
    use crate::bksystem::reputation::ParamsOfCalculate;
    use crate::bksystem::reputation::ReputationCoefficientCalculator;
    use crate::tests::create_context;

    #[tokio::test]
    async fn test_calculate() {
        let context = create_context();

        let contract = ReputationCoefficientCalculator::new(context);
        let coefficient = contract
            .calculate(ParamsOfCalculate { reputation_time: 97344 })
            .await
            .inspect_err(|e| eprintln!("Calculate ({e})"))
            .unwrap();
        assert_eq!(coefficient, 1008676871);
    }
}
