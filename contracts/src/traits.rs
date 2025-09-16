use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::DecodedMessageBody;
use tvm_client::abi::DeploySet;
use tvm_client::abi::ParamsOfDecodeMessage;
use tvm_client::abi::ParamsOfEncodeMessage;
use tvm_client::abi::ParamsOfEncodeMessageBody;
use tvm_client::abi::ResultOfEncodeMessage;
use tvm_client::abi::ResultOfEncodeMessageBody;
use tvm_client::abi::Signer;
use tvm_client::abi::{self};
use tvm_client::processing;
use tvm_client::processing::ParamsOfSendMessage;
use tvm_client::processing::ProcessingEvent;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::tvm;
use tvm_client::tvm::ParamsOfRunTvm;
use tvm_client::tvm::ResultOfRunTvm;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::account::ParamsOfWaitAccount;

pub trait AddressAccessor {
    fn address(&self) -> &str;
}

pub trait AbiAccessor {
    fn abi(&self) -> &Abi;
}

#[async_trait]
pub trait AccountAccessor: AsyncGuarded<Account> + AsyncGuardedMut<Account> {
    fn account(&self) -> &Arc<Mutex<Account>>;

    async fn wait_account(&self, params: ParamsOfWaitAccount) -> anyhow::Result<()> {
        self.async_guarded_mut(|mut account| async move { account.wait(params).await }).await
    }

    async fn fetch_account(&self) -> anyhow::Result<()> {
        self.async_guarded_mut(|mut account| async move { account.fetch().await }).await
    }

    async fn is_deployed(&self) -> bool {
        self.async_guarded(|account| account.is_deployed()).await
    }
}

pub trait ContextAccessor {
    fn context(&self) -> &Arc<ClientContext>;
}

pub trait DecodeMessage: ContextAccessor + AbiAccessor {
    fn decode_message(&self, boc: impl AsRef<str>) -> anyhow::Result<DecodedMessageBody> {
        abi::decode_message(
            self.context().clone(),
            ParamsOfDecodeMessage {
                abi: self.abi().clone(),
                message: boc.as_ref().to_string(),
                allow_partial: true,
                function_name: None,
                data_layout: None,
            },
        )
        .map_err(|e| anyhow!("Decode message ({e:?})"))
    }
}

#[async_trait]
pub trait EncodeMessage: ContextAccessor + AbiAccessor + AddressAccessor {
    async fn encode_message(
        &self,
        call_set: Option<CallSet>,
        deploy_set: Option<DeploySet>,
        signer: Signer,
    ) -> anyhow::Result<ResultOfEncodeMessage> {
        let params = ParamsOfEncodeMessage {
            abi: self.abi().clone(),
            address: Some(self.address().to_string()),
            deploy_set,
            call_set,
            signer,
            processing_try_index: None,
            signature_id: None,
        };
        abi::encode_message(self.context().clone(), params)
            .await
            .map_err(|e| anyhow!("Encode message ({e:?})"))
    }

    async fn encode_message_body(
        &self,
        call_set: CallSet,
        is_internal: bool,
        signer: Signer,
    ) -> anyhow::Result<ResultOfEncodeMessageBody> {
        let params = ParamsOfEncodeMessageBody {
            abi: self.abi().clone(),
            address: Some(self.address().to_string()),
            call_set,
            is_internal,
            signer,
            processing_try_index: None,
            signature_id: None,
        };
        abi::encode_message_body(self.context().clone(), params)
            .await
            .map_err(|e| anyhow!("Encode message body ({e:?})"))
    }
}

#[async_trait]
pub trait SendMessage: EncodeMessage {
    async fn send_message(
        &self,
        call_set: Option<CallSet>,
        deploy_set: Option<DeploySet>,
        signer: Signer,
    ) -> anyhow::Result<ResultOfSendMessage> {
        let encode_message_result = self.encode_message(call_set, deploy_set, signer).await?;
        let params = ParamsOfSendMessage {
            message: encode_message_result.message,
            abi: Some(self.abi().clone()),
            thread_id: None,
            send_events: false,
        };

        processing::send_message(self.context().clone(), params, process_message_callback)
            .await
            .map_err(|e| anyhow!("Send message ({e:?})"))
    }
}

#[async_trait]
pub trait Executor: EncodeMessage + AccountAccessor {
    async fn run_tvm(
        &self,
        call_set: Option<CallSet>,
        signer: Signer,
    ) -> anyhow::Result<ResultOfRunTvm> {
        self.fetch_account()
            .await
            .map_err(|e| anyhow!("Fetch account `{}` ({e})", self.address()))?;

        let account = self.async_guarded(|account| account.clone()).await;
        if !account.is_deployed() {
            anyhow::bail!("Account `{}` is not active", self.address())
        }

        let encode_message_result = self.encode_message(call_set, None, signer).await?;
        let params = ParamsOfRunTvm {
            message: encode_message_result.message.clone(),
            account: account.boc.unwrap(),
            execution_options: None,
            abi: Some(self.abi().clone()),
            boc_cache: None,
            return_updated_account: None,
        };

        tvm::run_tvm(self.context().clone(), params).await.map_err(|e| anyhow!("Run tvm ({e:?})"))
    }
}

async fn process_message_callback(event: ProcessingEvent) {
    tracing::debug!(target: "ackinacki_kit", "{event:?}");
}
