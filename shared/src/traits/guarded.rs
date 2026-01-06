use tokio::sync::OwnedMutexGuard;

pub trait AsyncGuarded<Inner> {
    fn async_guarded<F, T>(&self, action: F) -> impl Future<Output=T>
    where
        F: FnOnce(&Inner) -> T;
}

pub trait AsyncGuardedMut<Inner> {
    fn async_guarded_mut<F, Fut, T>(&self, action: F) -> impl Future<Output=anyhow::Result<T>>
    where
        F: FnOnce(OwnedMutexGuard<Inner>) -> Fut,
        Fut: Future<Output = anyhow::Result<T>>;
}
