use tvm_client::error::ClientError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KitModule {
    Token(TokenModule),
    Event,
    MvSystem(MvSystemModule),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenModule {
    Root,
    Wallet,
    Transaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MvSystemModule {
    Root,
    Multifactor,
    PopitGame,
    PopcoinWallet,
    PopcoinRoot,
    Mirror,
    Indexer,
    Boost,
    Miner,
    GameRoot,
}

impl From<TokenModule> for KitModule {
    fn from(value: TokenModule) -> Self {
        KitModule::Token(value)
    }
}

impl From<MvSystemModule> for KitModule {
    fn from(value: MvSystemModule) -> Self {
        KitModule::MvSystem(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KitErrorCode {
    None = -1,
    DeserializeFailed = 100,
    EmptyOutput = 101,
    EmptyResult = 102,
    Decode = 103,
    Convert = 104,

    // account
    FetchAccount = 200,
    AccountIsNotActive = 201,
    DecodeAccountData = 202,
    DeserializeAccountData = 203,
    WaitAccount = 204,

    // Event
    QueryEvents = 300,
    EmptyData = 301,
    Parse = 302,
    UnknownEvent = 303,
}

#[derive(Debug)]
pub struct KitError {
    pub tvm_error: Option<ClientError>,
    pub module: KitModule,
    pub code: KitErrorCode,
    pub message: String,
}

impl KitError {
    pub fn new(module: KitModule, code: KitErrorCode, message: impl Into<String>) -> Self {
        Self { tvm_error: None, module, code, message: message.into() }
    }

    pub fn with_tvm_error(mut self, err: ClientError) -> Self {
        self.tvm_error = Some(err);
        self
    }
}
