pub async fn sleep_ms(ms: u64) {
    sleep_impl(ms).await;
}

#[cfg(feature = "wasm")]
async fn sleep_impl(ms: u64) {
    use gloo_timers::future::TimeoutFuture;
    TimeoutFuture::new(ms as u32).await;
}

#[cfg(not(feature = "wasm"))]
async fn sleep_impl(ms: u64) {
    use tokio::time::sleep;
    use tokio::time::Duration;
    sleep(Duration::from_millis(ms)).await;
}
