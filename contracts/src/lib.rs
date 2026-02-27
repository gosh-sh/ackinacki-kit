pub mod account;
pub mod authservice;
pub mod bksystem;
pub mod deserialize;
pub mod dex;
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
    use tvm_client::processing;
    use tvm_client::processing::ParamsOfSendMessage;
    use tvm_client::ClientConfig;
    use tvm_client::ClientContext;

    use crate::traits::AccountAccessor;
    use crate::traits::AddressAccessor;

    pub const NETWORK_ENDPOINT: &str = "shellnet.ackinacki.org";

    pub fn create_context() -> Arc<ClientContext> {
        let mut config = ClientConfig::default();
        config.network.endpoints = Some(vec![NETWORK_ENDPOINT.to_string()]);

        let context = ClientContext::new(config).expect("Create context");
        Arc::new(context)
    }

    pub const NETWORK_GIVER_ADDRESS: &str =
        "0:1111111111111111111111111111111111111111111111111111111111111111";
    pub const NETWORK_GIVER_ABI_PATH: &str =
        "/Users/dronbas/Projects/ackinacki/acki-nacki/contracts/giver/GiverV3.abi.json";

    pub async fn giver_send_currency_with_flag(
        context: Arc<ClientContext>,
        dest: &str,
        native_value: u64,
        ecc: HashMap<u32, u64>,
        flag: u8,
    ) {
        let giver_abi = Abi::Json(
            std::fs::read_to_string(NETWORK_GIVER_ABI_PATH).expect("read GiverV3 ABI for tests"),
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
                address: Some(NETWORK_GIVER_ADDRESS.to_string()),
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
        async fn fetch_account_with_retry<T: AccountAccessor + AddressAccessor>(
            contract: &T,
            label: &str,
            phase: &str,
        ) {
            let max_attempts = 4;
            for attempt in 1..=max_attempts {
                match contract.fetch_account().await {
                    Ok(()) => return,
                    Err(err) => {
                        let msg = err
                            .tvm_error
                            .as_ref()
                            .map(|e| e.message.to_ascii_lowercase())
                            .unwrap_or_default();
                        let transient = msg.contains("connection reset by peer")
                            || msg.contains("client error (sendrequest)")
                            || msg.contains("all attempts failed");

                        if transient && attempt < max_attempts {
                            eprintln!(
                                "{label}: fetch_account {phase} transient network error on attempt {attempt}/{max_attempts}: {err:?}"
                            );
                            tokio::time::sleep(Duration::from_millis(700)).await;
                            continue;
                        }

                        panic!("{label}: fetch_account {phase} failed: {err:?}");
                    }
                }
            }
        }

        fetch_account_with_retry(contract, label, "before top-up check").await;
        let current_balance = {
            let guard = contract.account().lock().await;
            guard.balance.clone().unwrap_or_else(|| BigInt::from(0_u8))
        };

        let min_native = BigInt::from(min_native_balance);
        if current_balance >= min_native {
            return;
        }

        eprintln!(
            "{label} native balance {:?} is below {:?}; topping up via giver",
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
        fetch_account_with_retry(contract, label, "after top-up").await;

        let guard = contract.account().lock().await;
        eprintln!("{label} after top-up: balance={:?}, ecc={:?}", guard.balance, guard.ecc);
    }
}
