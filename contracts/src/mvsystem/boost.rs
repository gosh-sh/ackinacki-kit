use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::deserialize::deserialize_u64;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::DecodeMessage;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/Boost.abi.json");

#[derive(Debug)]
pub struct Boost {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl AccountAccessor for Boost {
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.account
    }
}

impl AbiAccessor for Boost {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Boost {
    fn address(&self) -> &str {
        &self.address
    }
}

impl ContextAccessor for Boost {
    fn context(&self) -> &Arc<ClientContext> {
        &self.context
    }
}

impl EncodeMessage for Boost {}

impl DecodeMessage for Boost {}

impl Executor for Boost {}

impl SendMessage for Boost {}

#[async_trait]
impl AsyncGuarded<Account> for Boost {
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
impl AsyncGuardedMut<Account> for Boost {
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

#[derive(Debug, Deserialize)]
pub struct ResultOfGetDetails {
    #[serde(rename = "mbiCur", deserialize_with = "deserialize_u64")]
    pub mamaboard_max_seq_no: u64,
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfSetMbiCur {
    #[serde(rename(serialize = "mbiCur"))]
    pub mamaboard_max_seq_no: u64,
}

impl Boost {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self {
            context: context.clone(),
            address: address.as_ref().to_string(),
            abi: Abi::Json(ABI.to_string()),
            account: Arc::new(Mutex::new(Account::new(context, address))),
        }
    }

    pub async fn get_details(&self) -> anyhow::Result<ResultOfGetDetails> {
        let call_set =
            CallSet { function_name: "getDetails".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetDetails>(value)
                    .map_err(|e| anyhow!("Deserialize output ({})", e)),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Set mamaboard max detail index
    ///
    /// Original contract method: `setMbiCur`
    ///
    /// Should be signed with root keys
    pub async fn set_mbi_cur(
        &self,
        params: ParamsOfSetMbiCur,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setMbiCur".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
