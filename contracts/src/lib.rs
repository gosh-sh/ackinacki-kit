pub mod account;
pub mod bksystem;
pub mod deserialize;
pub mod mvconfig;
pub mod mvsystem;
pub mod token;
pub mod traits;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tvm_client::ClientConfig;
    use tvm_client::ClientContext;

    pub fn create_context() -> Arc<ClientContext> {
        let mut config = ClientConfig::default();
        config.network.endpoints = Some(vec!["shellnet.ackinacki.org".to_string()]);

        let context = ClientContext::new(config).expect("Create context");
        Arc::new(context)
    }
}
