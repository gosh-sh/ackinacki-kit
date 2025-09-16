use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::EncodeMessage;
use crate::traits::Executor;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/Indexer.abi.json");

#[derive(Debug)]
pub struct Indexer {
    abi: Abi,
    account: Account,
}

impl AccountAccessor for Indexer {
    fn account(&self) -> &Account {
        &self.account
    }
}

impl AbiAccessor for Indexer {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Indexer {
    fn address(&self) -> &str {
        &self.account.address
    }
}

impl ContextAccessor for Indexer {
    fn context(&self) -> Arc<ClientContext> {
        self.account.context.clone()
    }
}

impl EncodeMessage for Indexer {}

impl Executor for Indexer {}

impl SendMessage for Indexer {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResultOfGetDetails {
    pub name: String,
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfSetOwner {
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfDeployMultifactor {
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
    pub zkid: String,
    pub proof: String,
    pub epk: String,
    pub epk_sig: String,
    pub epk_expire_at: u64,
    pub jwk_modulus: String,
    pub kid: String,
    pub jwk_modulus_expire_at: u64,
    pub index_mod_4: u8,
    pub iss_base_64: String,
    pub header_base_64: String,
    pub pub_recovery_key: String,
    pub pub_recovery_key_sig: String,
    pub root_provider_certificates: HashMap<String, String>,
    pub owner_pubkey: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfEncodeSetOwner {
    #[serde(rename = "wallet")]
    pub multifactor_address: String,
}

impl Indexer {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>) -> Self {
        Self { abi: Abi::Json(ABI.to_string()), account: Account::new(context, address) }
    }

    pub async fn get_details(&self) -> anyhow::Result<ResultOfGetDetails> {
        let call_set =
            CallSet { function_name: "getDetails".to_string(), header: None, input: None };

        let result = self.run_tvm(Some(call_set), Signer::None).await?;
        match result.decoded {
            Some(data) => match data.output {
                Some(value) => serde_json::from_value::<ResultOfGetDetails>(value)
                    .map_err(|e| anyhow!("Deserialize output ({e})")),
                None => anyhow::bail!("Empty decoded output"),
            },
            None => anyhow::bail!("Empty decoded result"),
        }
    }

    /// # Set indexer owner (multifactor) from root
    ///
    /// Original contract method: `setNewWalletRoot`
    ///
    /// Should be signed with root keys
    pub async fn set_owner(
        &self,
        params: ParamsOfSetOwner,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "setNewWalletRoot".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Encode set indexer owner message
    ///
    /// Original contract method: `setNewWallet`
    ///
    /// Should be sent from current owner multifactor
    pub async fn set_owner_message(
        &self,
        params: ParamsOfEncodeSetOwner,
    ) -> anyhow::Result<String> {
        let call_set = CallSet {
            function_name: "setNewWallet".to_string(),
            header: None,
            input: Some(json!(params)),
        };

        let result = self
            .encode_message_body(call_set, true, Signer::None)
            .await
            .map_err(|e| anyhow!("Encode message body ({e})"))?;

        Ok(result.body)
    }

    /// # Deploy multifactor
    ///
    /// Used to create multifactor and claim current indexer
    ///
    /// Original contract method: `isOwnerRoot`
    ///
    /// Should be signed with root keys
    pub async fn deploy_multifactor(
        &self,
        params: ParamsOfDeployMultifactor,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "isOwnerRoot".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
