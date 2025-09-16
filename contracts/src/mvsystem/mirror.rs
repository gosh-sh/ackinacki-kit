use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use num_bigint::BigUint;
use serde::Serialize;
use serde_json::json;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::Signer;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::mvsystem::PopitMedia;
use crate::traits::AbiAccessor;
use crate::traits::AccountAccessor;
use crate::traits::AddressAccessor;
use crate::traits::ContextAccessor;
use crate::traits::EncodeMessage;
use crate::traits::SendMessage;

const ABI: &str = include_str!("../../abi/mvsystem/Mirror.abi.json");

#[derive(Debug)]
pub struct Mirror {
    abi: Abi,
    account: Account,
}

impl AccountAccessor for Mirror {
    fn account(&self) -> &Account {
        &self.account
    }
}

impl AbiAccessor for Mirror {
    fn abi(&self) -> &Abi {
        &self.abi
    }
}

impl AddressAccessor for Mirror {
    fn address(&self) -> &str {
        &self.account.address
    }
}

impl ContextAccessor for Mirror {
    fn context(&self) -> Arc<ClientContext> {
        self.account.context.clone()
    }
}

impl EncodeMessage for Mirror {}

impl SendMessage for Mirror {}

#[derive(Debug, Serialize)]
pub struct ParamsOfDeployMultifactor {
    pub name: String,
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
    pub owner_pubkey: String,
    pub root_provider_certificates: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfDeployPopitgame {
    #[serde(rename = "multifactor")]
    pub multifactor_address: String,
}

#[derive(Debug, Serialize)]
pub struct ParamsOfDeployPopcoinRoot {
    pub name: String,
    #[serde(rename = "maxPopitIndex")]
    pub max_popit_index: u16,
    pub popits_media: HashMap<u16, PopitMedia>,
    #[serde(rename = "isPublic")]
    pub is_public: bool,
    pub description: Option<String>,
    #[serde(rename = "popitGameOwner")]
    pub owner_popitgame_address: String,
}

impl Mirror {
    pub fn new(context: Arc<ClientContext>, public: impl AsRef<str>) -> anyhow::Result<Self> {
        let public = {
            let bytes =
                hex::decode(public.as_ref()).map_err(|e| anyhow!("Decode hex to bytes ({e})"))?;
            BigUint::from_bytes_be(&bytes)
        };

        let address = {
            let index = (public % BigUint::from(1000_u32)) + BigUint::from(1_u32);
            format!("0:2{index:063x}")
        };

        Ok(Self { abi: Abi::Json(ABI.to_string()), account: Account::new(context, address) })
    }

    /// # Deploy multifactor account
    pub async fn deploy_multifactor(
        &self,
        params: ParamsOfDeployMultifactor,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployMultifactor".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy popitgame account
    pub async fn deploy_popitgame(
        &self,
        params: ParamsOfDeployPopitgame,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployPopitGame".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }

    /// # Deploy popcoin root account
    pub async fn deploy_popcoin_root(
        &self,
        params: ParamsOfDeployPopcoinRoot,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let call_set = CallSet {
            function_name: "deployPopCoinRoot".to_string(),
            header: None,
            input: Some(json!(params)),
        };
        self.send_message(Some(call_set), None, signer).await
    }
}
