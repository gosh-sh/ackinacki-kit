use async_trait::async_trait;
use tokio::sync::OwnedMutexGuard;

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
pub trait AsyncGuarded<Inner> {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Inner) -> T + 'async_trait,
        T: 'async_trait;
}

#[cfg_attr(feature = "wasm", async_trait(?Send))]
#[cfg_attr(not(feature = "wasm"), async_trait)]
pub trait AsyncGuardedMut<Inner> {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Inner>) -> Fut + 'async_trait,
        Fut: Future<Output = anyhow::Result<T>> + 'async_trait,
        T: 'async_trait;
}
