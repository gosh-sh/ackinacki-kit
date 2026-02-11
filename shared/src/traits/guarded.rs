use tokio::sync::OwnedMutexGuard;

pub trait AsyncGuarded<Inner> {
    fn async_guarded<F, T>(&self, action: F) -> impl Future<Output = T>
    where
        F: FnOnce(&Inner) -> T;
}

pub trait AsyncGuardedMut<Inner> {
    fn async_guarded_mut<F, Fut, T, E>(&self, action: F) -> impl Future<Output = Result<T, E>>
    where
        F: FnOnce(OwnedMutexGuard<Inner>) -> Fut,
        Fut: Future<Output = Result<T, E>>;
}
