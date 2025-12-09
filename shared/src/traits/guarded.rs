use async_trait::async_trait;
use tokio::sync::OwnedMutexGuard;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait AsyncGuarded<Inner> {
    async fn async_guarded<F, T>(&self, action: F) -> T
    where
        F: FnOnce(&Inner) -> T + Send + 'async_trait,
        T: Send + 'async_trait;
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait AsyncGuardedMut<Inner> {
    async fn async_guarded_mut<F, Fut, T>(&self, action: F) -> anyhow::Result<T>
    where
        F: FnOnce(OwnedMutexGuard<Inner>) -> Fut + Send + 'async_trait,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'async_trait,
        T: Send + 'async_trait;
}
