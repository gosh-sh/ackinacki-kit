use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tvm_client::error::ClientResult;
use tvm_client::processing;
use tvm_client::processing::ParamsOfSendMessage;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::ClientContext;

use crate::traits::send_message_callback;

/// An ABI-encoded and signed message that can be submitted more than once
/// without changing its identity.
#[derive(Debug, Clone)]
pub struct PreparedMessage {
    params: ParamsOfSendMessage,
    message_id: String,
    expires_at: Option<u32>,
}

impl PreparedMessage {
    pub(crate) fn new(
        params: ParamsOfSendMessage,
        message_id: String,
        expires_at: Option<u32>,
    ) -> Self {
        Self { params, message_id, expires_at }
    }

    pub fn message(&self) -> &str {
        &self.params.message
    }

    pub fn message_id(&self) -> &str {
        &self.message_id
    }

    pub fn expires_at(&self) -> Option<u32> {
        self.expires_at
    }

    /// Submit this exact BOC once. Consumers may call this repeatedly only
    /// when their delivery policy explicitly permits it.
    pub async fn send_once(
        &self,
        context: Arc<ClientContext>,
    ) -> ClientResult<ResultOfSendMessage> {
        processing::send_message(context, self.params.clone(), send_message_callback).await
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait PreparedMessageSender: fmt::Debug + Send + Sync {
    async fn send(
        &self,
        context: Arc<ClientContext>,
        message: &PreparedMessage,
    ) -> ClientResult<ResultOfSendMessage>;
}

#[derive(Debug, Default)]
pub struct DirectMessageSender;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl PreparedMessageSender for DirectMessageSender {
    async fn send(
        &self,
        context: Arc<ClientContext>,
        message: &PreparedMessage,
    ) -> ClientResult<ResultOfSendMessage> {
        message.send_once(context).await
    }
}

/// Runtime dependencies shared by contract wrappers.
///
/// The default conversion from `Arc<ClientContext>` preserves the historical
/// direct-send behavior. Applications that need delivery policy can inject a
/// sender and an explicit message lifetime without putting that policy in kit.
#[derive(Clone)]
pub struct ContractContext {
    client: Arc<ClientContext>,
    sender: Arc<dyn PreparedMessageSender>,
    message_lifetime: Option<Duration>,
}

impl ContractContext {
    pub fn new(client: Arc<ClientContext>) -> Self {
        Self { client, sender: Arc::new(DirectMessageSender), message_lifetime: None }
    }

    pub fn with_sender(
        client: Arc<ClientContext>,
        sender: Arc<dyn PreparedMessageSender>,
        message_lifetime: Duration,
    ) -> Self {
        Self { client, sender, message_lifetime: Some(message_lifetime) }
    }

    pub fn client(&self) -> &Arc<ClientContext> {
        &self.client
    }

    pub fn sender(&self) -> &Arc<dyn PreparedMessageSender> {
        &self.sender
    }

    pub fn message_lifetime(&self) -> Option<Duration> {
        self.message_lifetime
    }
}

impl fmt::Debug for ContractContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContractContext")
            .field("client", &self.client)
            .field("sender", &self.sender)
            .field("message_lifetime", &self.message_lifetime)
            .finish()
    }
}

impl From<Arc<ClientContext>> for ContractContext {
    fn from(client: Arc<ClientContext>) -> Self {
        Self::new(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_message_keeps_the_exact_encoded_boc() {
        let message = PreparedMessage::new(
            ParamsOfSendMessage {
                message: "encoded-and-signed-boc".to_string(),
                ..Default::default()
            },
            "message-id".to_string(),
            Some(42),
        );

        let cloned = message.clone();
        assert_eq!(message.message(), "encoded-and-signed-boc");
        assert_eq!(cloned.message(), message.message());
        assert_eq!(cloned.message_id(), message.message_id());
        assert_eq!(cloned.expires_at(), Some(42));
    }

    #[test]
    fn configured_context_retains_the_explicit_lifetime() {
        let client = Arc::new(ClientContext::new(Default::default()).unwrap());
        let sender: Arc<dyn PreparedMessageSender> = Arc::new(DirectMessageSender);
        let context = ContractContext::with_sender(client, sender.clone(), Duration::from_secs(30));

        assert_eq!(context.message_lifetime(), Some(Duration::from_secs(30)));
        assert!(Arc::ptr_eq(context.sender(), &sender));
    }
}
