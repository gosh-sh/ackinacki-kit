pub mod account;
pub mod accumulator;
pub mod authservice;
pub mod bksystem;
pub mod dapp;
pub mod deserialize;
pub mod error;
pub mod event;
pub mod exchange;
pub mod giver;
pub mod multisig;
pub mod mvconfig;
pub mod mvsystem;
pub mod token;
pub mod traits;

pub type KitResult<T> = Result<T, error::KitError>;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tvm_client::ClientConfig;
    use tvm_client::ClientContext;

    pub const NETWORK_ENDPOINT: &str = "shellnet.ackinacki.org";

    /// Live testnet multifactor account. Update here if it goes stale.
    pub const MULTIFACTOR_ADDRESS: &str =
        "0:476a7b48ac45f2a57a57cea42bc2693a24c9f2ad06bdd5b2028d92ecb7a9db4c";
    pub const MULTIFACTOR_EPK: &str =
        "6d26db3f0d23f66f358ca7d8f4e340ecc784f899002946b4eb04b1f7cb3325d6";
    pub const MULTIFACTOR_ESK: &str =
        "15910e12c0bc445cda49ad240a9533546a8c26b8a8d0313cd59533af1b463bc7";
    pub const MULTIFACTOR_EPK_EXPIRE_AT: u64 = 1_784_029_474;

    pub fn create_context() -> Arc<ClientContext> {
        let mut config = ClientConfig::default();
        config.network.endpoints = Some(vec![NETWORK_ENDPOINT.to_string()]);

        let context = ClientContext::new(config).expect("Create context");
        Arc::new(context)
    }
}
