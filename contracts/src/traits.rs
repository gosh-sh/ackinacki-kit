use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use shared::traits::guarded::AsyncGuarded;
use shared::traits::guarded::AsyncGuardedMut;
use tokio::sync::Mutex;
use tvm_client::abi::Abi;
use tvm_client::abi::CallSet;
use tvm_client::abi::DecodedMessageBody;
use tvm_client::abi::DeploySet;
use tvm_client::abi::ParamsOfDecodeAccountData;
use tvm_client::abi::ParamsOfDecodeMessage;
use tvm_client::abi::ParamsOfEncodeMessage;
use tvm_client::abi::ParamsOfEncodeMessageBody;
use tvm_client::abi::ResultOfEncodeMessage;
use tvm_client::abi::ResultOfEncodeMessageBody;
use tvm_client::abi::Signer;
use tvm_client::abi::{self};
use tvm_client::processing;
use tvm_client::processing::ParamsOfSendMessage;
use tvm_client::processing::ProcessingEvent;
use tvm_client::processing::ResultOfSendMessage;
use tvm_client::tvm;
use tvm_client::tvm::ParamsOfRunTvm;
use tvm_client::tvm::ResultOfRunTvm;
use tvm_client::ClientContext;

use crate::account::Account;
use crate::account::ParamsOfWaitAccount;
use crate::error::KitError;
use crate::error::KitErrorCode;
use crate::error::KitModule;
use crate::event::Event;
use crate::KitResult;

pub trait ModuleAccessor {
    const MODULE: KitModule;
}

/// Shared storage for contract wrappers that keeps the repetitive runtime
/// dependencies in one place (`context`, `address`, `abi`, `account`).
///
/// Migration pattern (incremental, module-by-module):
/// 1. Replace per-contract fields with `base: ContractBase`.
/// 2. Implement `HasContractBase` for the wrapper.
/// 3. Keep `ModuleAccessor` explicit (module identity is contract-specific).
/// 4. Opt into blanket message/executor impls via `AutoContract`.
///
/// This allows reducing boilerplate without forcing a repo-wide refactor.
#[derive(Debug, Clone)]
pub struct ContractBase {
    context: Arc<ClientContext>,
    address: String,
    abi: Abi,
    account: Arc<Mutex<Account>>,
}

impl ContractBase {
    pub fn new(context: Arc<ClientContext>, address: impl AsRef<str>, abi: Abi) -> Self {
        let address = address.as_ref().to_string();
        Self {
            account: Arc::new(Mutex::new(Account::new(context.clone(), &address))),
            context,
            address,
            abi,
        }
    }
}

pub trait HasContractBase {
    fn base(&self) -> &ContractBase;
}

pub trait AddressAccessor {
    fn address(&self) -> &str;
}

impl<T> AddressAccessor for T
where
    T: HasContractBase,
{
    fn address(&self) -> &str {
        &self.base().address
    }
}

pub trait AbiAccessor {
    fn abi(&self) -> &Abi;
}

impl<T> AbiAccessor for T
where
    T: HasContractBase,
{
    fn abi(&self) -> &Abi {
        &self.base().abi
    }
}

pub trait AccountAccessor:
    ModuleAccessor + AsyncGuarded<Account> + AsyncGuardedMut<Account>
{
    fn account(&self) -> &Arc<Mutex<Account>>;

    fn wait_account(&self, params: ParamsOfWaitAccount) -> impl Future<Output = KitResult<()>> {
        async {
            self.async_guarded_mut(|mut account| async move { account.wait(params).await }).await
        }
    }

    fn fetch_account(&self) -> impl Future<Output = KitResult<()>> {
        async { self.async_guarded_mut(|mut account| async move { account.fetch().await }).await }
    }

    fn is_deployed(&self) -> impl Future<Output = bool> {
        async {
            let _ = self.fetch_account().await;
            self.async_guarded(|account| account.is_deployed()).await
        }
    }
}

impl<T> AccountAccessor for T
where
    T: ModuleAccessor + HasContractBase + AsyncGuarded<Account> + AsyncGuardedMut<Account>,
{
    fn account(&self) -> &Arc<Mutex<Account>> {
        &self.base().account
    }
}

// impl<T> AsyncGuarded<Account> for T
// where
//     T: AccountAccessor + Send + Sync,
// {
//     fn async_guarded<F, R>(&self, action: F) -> impl Future<Output = R>
//     where
//         F: FnOnce(&Account) -> R,
//     {
//         let account: Arc<Mutex<Account>> = self.account().clone();
//         async move {
//             let guard = account.lock().await;
//             action(&guard)
//         }
//     }
// }

// impl<T> AsyncGuardedMut<Account> for T
// where
//     T: AccountAccessor + Send + Sync,
// {
//     fn async_guarded_mut<F, Fut, R, E>(&self, action: F) -> impl Future<Output = Result<R, E>>
//     where
//         F: FnOnce(OwnedMutexGuard<Account>) -> Fut,
//         Fut: Future<Output = Result<R, E>>,
//     {
//         let account = self.account().clone();
//         async move {
//             let guard = account.lock_owned().await;
//             action(guard).await
//         }
//     }
// }

pub trait ContextAccessor {
    fn context(&self) -> &Arc<ClientContext>;
}

impl<T> ContextAccessor for T
where
    T: HasContractBase,
{
    fn context(&self) -> &Arc<ClientContext> {
        &self.base().context
    }
}

/// Opt-in marker for gradual migration to blanket impls (`EncodeMessage`,
/// `DecodeMessage`, `Executor`, `SendMessage`, `VersionAccessor`,
/// `DecodeAccountData`) without conflicting with existing explicit impls
/// in modules that have not been migrated yet.
pub trait AutoContract:
    ModuleAccessor + ContextAccessor + AbiAccessor + AddressAccessor + AccountAccessor
{
}

pub trait DecodeAccountData<T: DeserializeOwned>:
    ModuleAccessor + ContextAccessor + AbiAccessor
{
    fn decode_account_data(&self, data: impl AsRef<str>) -> KitResult<T> {
        let value = abi::decode_account_data(
            self.context().clone(),
            ParamsOfDecodeAccountData {
                abi: self.abi().clone(),
                data: data.as_ref().to_string(),
                allow_partial: true,
            },
        )
        .map_err(|e| {
            KitError::new(
                Self::MODULE,
                KitErrorCode::DecodeAccountData,
                format!("Decode account data ({e:?})"),
            )
        })?
        .data;

        serde_json::from_value::<T>(value).map_err(|e| {
            KitError::new(
                Self::MODULE,
                KitErrorCode::DeserializeAccountData,
                format!("Deserialize account data ({e:?})"),
            )
        })
    }
}

impl<C, T> DecodeAccountData<T> for C
where
    C: AutoContract,
    T: DeserializeOwned,
{
}

pub trait DecodeMessage: ModuleAccessor + ContextAccessor + AbiAccessor {
    fn decode_message(&self, boc: impl AsRef<str>) -> KitResult<DecodedMessageBody> {
        abi::decode_message(
            self.context().clone(),
            ParamsOfDecodeMessage {
                abi: self.abi().clone(),
                message: boc.as_ref().to_string(),
                allow_partial: true,
                function_name: None,
                data_layout: None,
            },
        )
        .map_err(|e| {
            KitError::new(Self::MODULE, KitErrorCode::None, format!("Decode message ({e:?})"))
                .with_tvm_error(e)
        })
    }
}

impl<C> DecodeMessage for C
where
    C: AutoContract,
{
}

pub trait EncodeMessage: ModuleAccessor + ContextAccessor + AbiAccessor + AddressAccessor {
    fn encode_message(
        &self,
        call_set: Option<CallSet>,
        deploy_set: Option<DeploySet>,
        signer: Signer,
    ) -> impl Future<Output = KitResult<ResultOfEncodeMessage>> {
        async {
            let params = ParamsOfEncodeMessage {
                abi: self.abi().clone(),
                address: Some(self.address().to_string()),
                deploy_set,
                call_set,
                signer,
                processing_try_index: None,
                signature_id: None,
            };
            abi::encode_message(self.context().clone(), params).await.map_err(|e| {
                KitError::new(Self::MODULE, KitErrorCode::None, "Encode message").with_tvm_error(e)
            })
        }
    }

    fn encode_message_body(
        &self,
        call_set: CallSet,
        is_internal: bool,
        signer: Signer,
    ) -> impl Future<Output = KitResult<ResultOfEncodeMessageBody>> {
        async move {
            let params = ParamsOfEncodeMessageBody {
                abi: self.abi().clone(),
                address: Some(self.address().to_string()),
                call_set,
                is_internal,
                signer,
                processing_try_index: None,
                signature_id: None,
            };
            abi::encode_message_body(self.context().clone(), params).await.map_err(|e| {
                KitError::new(Self::MODULE, KitErrorCode::None, "Encode message body")
                    .with_tvm_error(e)
            })
        }
    }
}

impl<C> EncodeMessage for C
where
    C: AutoContract,
{
}

pub trait SendMessage: ModuleAccessor + EncodeMessage {
    fn send_message(
        &self,
        call_set: Option<CallSet>,
        deploy_set: Option<DeploySet>,
        signer: Signer,
    ) -> impl Future<Output = KitResult<ResultOfSendMessage>> {
        async {
            let encode_message_result = self.encode_message(call_set, deploy_set, signer).await?;
            let params = ParamsOfSendMessage {
                message: encode_message_result.message,
                abi: Some(self.abi().clone()),
                thread_id: None,
                send_events: false,
            };

            processing::send_message(self.context().clone(), params, process_message_callback)
                .await
                .map_err(|e| {
                    KitError::new(Self::MODULE, KitErrorCode::None, "Send message")
                        .with_tvm_error(e)
                })
        }
    }
}

impl<C> SendMessage for C
where
    C: AutoContract,
{
}

pub trait Executor: EncodeMessage + AccountAccessor {
    fn run_tvm(
        &self,
        call_set: Option<CallSet>,
        signer: Signer,
    ) -> impl Future<Output = KitResult<ResultOfRunTvm>> {
        async {
            self.fetch_account().await?;

            let account = self.async_guarded(|account| account.clone()).await;
            if !account.is_deployed() {
                return Err(KitError::new(
                    Self::MODULE,
                    KitErrorCode::AccountIsNotActive,
                    format!("Account `{}` is not active", self.address()),
                ));
            }

            let encode_message_result = self.encode_message(call_set, None, signer).await?;
            let params = ParamsOfRunTvm {
                message: encode_message_result.message.clone(),
                account: account.boc.unwrap(),
                execution_options: None,
                abi: Some(self.abi().clone()),
                boc_cache: None,
                return_updated_account: None,
            };

            tvm::run_tvm(self.context().clone(), params).await.map_err(|e| {
                KitError::new(Self::MODULE, KitErrorCode::None, "Run tvm").with_tvm_error(e)
            })
        }
    }
}

impl<C> Executor for C
where
    C: AutoContract,
{
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultOfGetVersion {
    #[serde(rename = "value0")]
    pub version: String,
    #[serde(rename = "value1")]
    pub contract_name: String,
}

pub trait VersionAccessor: GetMethodAccessor + Executor {
    fn get_version(&self) -> impl Future<Output = KitResult<ResultOfGetVersion>> {
        async { self.call_get_method::<ResultOfGetVersion>("getVersion").await }
    }
}

impl<C> VersionAccessor for C
where
    C: AutoContract + GetMethodAccessor + Executor,
{
}

pub trait FromEvent {
    fn from_event(event: &Event, contract: &impl DecodeMessage) -> KitResult<Self>
    where
        Self: Sized;
}

#[derive(Debug, Clone)]
pub struct GetMethodArgs<I = ()> {
    pub function_name: &'static str,
    pub input: Option<I>,
}

impl GetMethodArgs<()> {
    pub const fn new(function_name: &'static str) -> Self {
        Self { function_name, input: None }
    }
}

impl<I> GetMethodArgs<I> {
    pub fn input<J>(self, input: J) -> GetMethodArgs<J> {
        GetMethodArgs { function_name: self.function_name, input: Some(input) }
    }
}

pub trait GetMethodAccessor: ModuleAccessor + Executor {
    fn call_get_method<T>(
        &self,
        name: &'static str,
    ) -> impl std::future::Future<Output = KitResult<T>>
    where
        T: DeserializeOwned,
    {
        self.get_method::<T, _>(GetMethodArgs::new(name))
    }

    fn call_get_method_with<T, I>(
        &self,
        name: &'static str,
        input: I,
    ) -> impl std::future::Future<Output = KitResult<T>>
    where
        T: DeserializeOwned,
        I: Serialize,
    {
        self.get_method::<T, _>(GetMethodArgs::new(name).input(input))
    }

    fn get_method<T, I>(
        &self,
        args: GetMethodArgs<I>,
    ) -> impl std::future::Future<Output = KitResult<T>>
    where
        T: DeserializeOwned,
        I: Serialize,
    {
        async move {
            let call_set = CallSet {
                function_name: args.function_name.to_string(),
                header: None,
                input: args.input.map(|i| json!(i)),
            };

            let result = self.run_tvm(Some(call_set), Signer::None).await?;

            let decoded = result.decoded.ok_or_else(|| {
                KitError::new(Self::MODULE, KitErrorCode::EmptyResult, "Empty decoded result")
            })?;

            let output = decoded.output.ok_or_else(|| {
                KitError::new(Self::MODULE, KitErrorCode::EmptyOutput, "Empty decoded output")
            })?;

            serde_json::from_value::<T>(output).map_err(|e| {
                KitError::new(
                    KitModule::from(Self::MODULE),
                    KitErrorCode::DeserializeFailed,
                    format!("Deserialize output ({e})"),
                )
            })
        }
    }
}

impl<T> GetMethodAccessor for T where T: ModuleAccessor + Executor {}

async fn process_message_callback(event: ProcessingEvent) {
    tracing::debug!(target: "ackinacki_kit", "{event:?}");
}
