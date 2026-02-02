use tvm_client::error::ClientError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KitModule {
    Token(TokenModule),
    Event,
    Account,
    MvSystem(MvSystemModule),
    BkSystem(BkSystemModule),
    MvConfig,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BkSystemModule {
    BlockKeeperWallet,
    BlockManagerWallet,
    Reputation,
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

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KitErrorCode {
    None = -1,
    DeserializeFailed = 100,
    EmptyOutput = 101,
    EmptyResult = 102,
    Decode = 103,
    Convert = 104,

    // account
    AccountIsNotActive = 200,
    DecodeAccountData = 201,
    DeserializeAccountData = 202,
    WaitAccount = 203,
    EncodeAccount = 204,
    GetAccount = 205,
    ConstructAccount = 206,
    ParseAccount = 207,
    IterateCurrencies = 208,

    // Event
    QueryEvents = 300,
    EmptyData = 301,
    Parse = 302,
    UnknownEvent = 303,
}

pub fn as_i32(self) -> i32 {
    self as i32
}

pub fn as_string(self) -> String {
    (self as i32).to_string()
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
