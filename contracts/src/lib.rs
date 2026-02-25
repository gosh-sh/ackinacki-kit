pub mod account;
pub mod authservice;
pub mod bksystem;
pub mod dex;
pub mod deserialize;
pub mod error;
pub mod event;
pub mod mvconfig;
pub mod mvsystem;
pub mod token;
pub mod traits;

pub type KitResult<T> = Result<T, error::KitError>;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use num_bigint::BigInt;
    use serde_json::json;
    use tvm_client::abi::Abi;
    use tvm_client::abi::CallSet;
    use tvm_client::ClientConfig;
    use tvm_client::ClientContext;
    use tvm_client::processing;
    use tvm_client::processing::ParamsOfSendMessage;

    use crate::traits::AccountAccessor;
    use crate::traits::AddressAccessor;

    pub fn create_context() -> Arc<ClientContext> {
        let mut config = ClientConfig::default();
        config.network.endpoints = Some(vec!["shellnet.ackinacki.org".to_string()]);

        let context = ClientContext::new(config).expect("Create context");
        Arc::new(context)
    }

    pub const SHELLNET_GIVER_ADDRESS: &str =
        "0:1111111111111111111111111111111111111111111111111111111111111111";
    pub const SHELLNET_GIVER_ABI_PATH: &str =
        "/Users/dronbas/Projects/ackinacki/acki-nacki/contracts/giver/GiverV3.abi.json";

    pub async fn giver_send_currency_with_flag(
        context: Arc<ClientContext>,
        dest: &str,
        native_value: u64,
        ecc: HashMap<u32, u64>,
        flag: u8,
    ) {
        let giver_abi = Abi::Json(
            std::fs::read_to_string(SHELLNET_GIVER_ABI_PATH).expect("read GiverV3 ABI for tests"),
        );

        let call_set = CallSet {
            function_name: "sendCurrencyWithFlag".to_string(),
            header: None,
            input: Some(json!({
                "dest": dest,
                "value": native_value,
                "ecc": ecc,
                "flag": flag,
            })),
        };

        let encoded = tvm_client::abi::encode_message(
            context.clone(),
            tvm_client::abi::ParamsOfEncodeMessage {
                abi: giver_abi.clone(),
                address: Some(SHELLNET_GIVER_ADDRESS.to_string()),
                deploy_set: None,
                call_set: Some(call_set),
                signer: tvm_client::abi::Signer::None,
                processing_try_index: None,
                signature_id: None,
            },
        )
        .await
        .expect("encode GiverV3.sendCurrencyWithFlag");

        processing::send_message(
            context,
            ParamsOfSendMessage {
                message: encoded.message,
                abi: Some(giver_abi),
                thread_id: None,
                send_events: false,
            },
            |_| Box::pin(async {}),
        )
        .await
        .expect("send GiverV3.sendCurrencyWithFlag");
    }

    pub async fn top_up_native_with_giver_if_below<T>(
        context: Arc<ClientContext>,
        contract: &T,
        min_native_balance: u64,
        top_up_native_value: u64,
        label: &str,
    ) where
        T: AccountAccessor + AddressAccessor,
    {
        contract.fetch_account().await.expect("fetch account before top-up check");
        let current_balance = {
            let guard = contract.account().lock().await;
            guard.balance.clone().unwrap_or_else(|| BigInt::from(0_u8))
        };

        let min_native = BigInt::from(min_native_balance);
        if current_balance >= min_native {
            return;
        }

        eprintln!(
            "{label} native balance {:?} is below {:?}; topping up via shellnet giver",
            current_balance, min_native
        );

        giver_send_currency_with_flag(
            context,
            contract.address(),
            top_up_native_value,
            HashMap::new(),
            1,
        )
        .await;

        tokio::time::sleep(Duration::from_secs(3)).await;
        contract.fetch_account().await.expect("fetch account after top-up");

        let guard = contract.account().lock().await;
        eprintln!(
            "{label} after top-up: balance={:?}, ecc={:?}",
            guard.balance, guard.ecc
        );
    }
}
