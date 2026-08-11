# Changelog

All notable changes to `ackinacki-kit` are documented here. The format loosely
follows [Keep a Changelog](https://keepachangelog.com/); the workspace is
versioned as a whole (`package.version` in the root `Cargo.toml`).

## [Unreleased]

### Fixed
- `Account::balance` now decodes the GraphQL hexadecimal representation of TVM
  `Grams` as an unsigned magnitude. The public type remains `Option<BigInt>`,
  but values whose most-significant byte starts with bit `1` are no longer
  misreported as negative two's-complement numbers.

## [5.1.0]

The `multisig` binding now targets `UpdateCustodianMultisigWallet_v2` **v2.4.0**.
The bundled ABI/TVC under `contracts/abi/multisig/` are re-vendored verbatim from
`gosh-sh/acki-nacki` (`dev`, commit `44fe02ea`,
`contracts/0.81.0_compiled/updatecustodianmultisigwallet_v2/`), replacing the
`6ad89549` build shipped in `5.0.0`. Nothing older is supported: the wallet
gained a fourth request queue and a stored balance config, so storage layout,
constructor and event set all differ. Wallets whose code hash is not `CODE_HASH`
must be driven by a binding that matches them.

### Added
- Balance config (gas self-management) — the wallet converts SHELL to vmshell
  when its balance drops below `minBalance`, up to `targetBalance`;
  `minBalance == 0` disables it, and the config survives custodian changes.
  Bound as `submit_config_update` / `confirm_config_update` (with
  `ParamsOfSubmitConfigUpdate`, `ResultOfSubmitConfigUpdate`,
  `ParamsOfConfirmConfigUpdate`) plus `get_balance_config`, `get_config_update`,
  `get_config_updates` and `get_config_update_ids`. New `BalanceConfig` and
  `ConfigUpdate` types.
- Custodian-set updates — `submit_data_update` / `confirm_data_update` and the
  queue reads `get_update_data`, `get_update_datas`, `get_update_data_ids`.
  Applying one now clears *all four* queues (transfers, data, code, config).
- Code-update queue reads — `get_update_code` (with the pending cells),
  `get_update_codes` (each pending code identified by hash, as the contract's
  listing get-method does; the hashes are computed from the stored cells) and
  `get_update_code_ids`. New `CodeUpdateInfo` type.
- `set_max_cleanup_operations` and `get_max_cleanup_operations` — the
  expired-request cleanup budget, with `MIN_CLEANUP_OPERATIONS` /
  `DEFAULT_CLEANUP_OPERATIONS` mirrored from the contract.
- `is_confirmed(mask, index)` — the contract's `isConfirmed`, as a pure
  function; and `expiration_bound(now)` + `ZERO_TIME`, the request-id expiry
  arithmetic behind the list-shaped reads.
- `AccountData` gains the v2.4 fields `requests_mask_config`, `config_updates`
  and `balance_config`.

### Changed
- `CODE_HASH` is now
  `cfcaac10d43c8dc062298cb48df097be67cddec52b9cfd558309a7549f01c1f1`, `VERSION`
  is `2.4.0`, and the vendored TVC's sha256 changed. Deploys must pair this TVC
  with this ABI (on ABI ≥ 2.3 the address depends on the ABI's `fields` list).
- The v2.4 constructor takes two more arguments (`minBalance`, `targetBalance`).
  Deploy stays out of scope here, but a deployer's call has to grow with it.
- The list-shaped reads (`get_transactions`, `get_transaction_ids`, and the new
  `get_update_datas` / `get_update_codes` / `get_config_updates` and their id
  forms) drop expired requests, the way the contract's listing get-methods do.
  Since expiry is derived from the request id and there is no block clock
  off-chain, the bound is computed from the *client's* clock. The by-id reads
  still return an expired request, also matching the contract, and
  `account_data()` exposes every queue unfiltered.
- `get_custodians` is ordered by the stored `hash(pubkey, address)` map key —
  which is what the on-chain dictionary is keyed by, and what its iteration
  order follows. The `5.0.0` entry described this as ordering by
  `owner_pubkey`; the order itself is unchanged.
- Constant docs: `MAX_QUEUED_TRANSACTIONS` (contract `MAX_QUEUED_REQUESTS`) caps
  unconfirmed requests per custodian *per queue*, all four queues separately.

### Unchanged
- `submit_transaction`, `send_transaction`, `confirm_transaction`,
  `submit_update_code`, `confirm_update_code` and their params, including
  `dapp_id` (still stored for off-chain use and not used to address the
  outbound message).
- Reads still decode the account's data cell instead of executing get-methods:
  a get-method call still fails in `run_tvm` with `code 404 TVM internal error:
  can not parse actions: 0`, because `tvm_block`'s action-list parser does not
  recognise a tag emitted by `sol 0.81.0` output. This build comes from the same
  compiler as the one that was verified on shellnet.
- Events are still not bound. The wallet's lifecycle events (now 15, ids
  `1100`–`1116`) can be decoded through `crate::event` with this ABI.

## [5.0.0]

The `multisig` binding now targets `UpdateCustodianMultisigWallet_v2`
exclusively. The bundled ABI/TVC under `contracts/abi/multisig/` are vendored
verbatim from `gosh-sh/acki-nacki` (`dev`, commit `6ad89549`,
`contracts/0.81.0_compiled/updatecustodianmultisigwallet_v2/`), replacing the
older flat `Multisig` build.

### Added
- `submit_update_code` / `confirm_update_code` — the v2-only code upgrade,
  queued and confirmed like a transaction, with `ParamsOfSubmitUpdateCode`,
  `ResultOfSubmitUpdateCode` and `ParamsOfConfirmUpdateCode`.
- `Multisig::account_data()` — fetches the account and decodes its persistent
  storage. This is the single round-trip behind every read method; call it
  directly when you need more than one of them.
- Storage now decoded in full: `AccountData` gains `transactions`,
  `data_updates`, `code_updates` and `custodians` (all `BTreeMap`, so iteration
  order is stable) plus the v2 field `requests_mask_code`. New `DataUpdate` and
  `CodeUpdate` types.
- Contract constants mirrored from the vendored build, since they live in code
  and not in the data cell: `MAX_QUEUED_TRANSACTIONS`, `MAX_CUSTODIAN_COUNT`,
  `EXPIRATION_TIME`, `VERSION`, `CONTRACT_NAME`, and `CODE_HASH` (the code hash
  a node reports for accounts deployed from the bundled TVC).
- Tests pinning the vendored assets (TVC sha256 + code hash + `sol 0.81.0`),
  asserting the ABI is the v2 build, and decoding the TVC's initial data cell
  through `AccountData` so the storage layout is checked without a network.

### Changed (breaking)
- `get_version` is no longer a network call: it is now a synchronous
  `fn get_version(&self) -> ResultOfGetVersion` answering from the vendored
  constants. It describes the bundled assets, not the code deployed at the
  wallet's address — compare the account's code hash against `CODE_HASH` to
  confirm the two agree.
- `multisig::ResultOfGetVersion` is gone; the binding reuses
  `traits::ResultOfGetVersion`, whose fields are `version` / `contract_name`
  (the removed local type had `kind` / `version`).
- Every read method (`get_parameters`, `get_custodians`, `get_transactions`,
  `get_transaction`, `get_transaction_ids`) now decodes the account's data cell
  instead of executing a get-method — `Multisig` no longer goes through
  `GetMethodAccessor`. Running any get-method against the v2 code fails in
  `run_tvm` with `code 404 TVM internal error: can not parse actions: 0`,
  because `tvm_block`'s action-list parser does not recognise a tag emitted by
  `sol 0.81.0` output (identical in tvm-sdk 3.0.2 and 3.0.4, so bumping the SDK
  does not help). Verified on shellnet for all six read methods. Writes are
  unaffected — they go through `process_message`.
- Consequences of reading storage: `get_parameters` answers
  `maxQueuedTransactions` / `maxCustodianCount` / `expirationTime` from the
  mirrored constants above; `get_transaction` now errors with
  `KitErrorCode::EmptyResult` when the id is not queued (the contract throws
  `102` in the same case); `get_transactions` / `get_transaction_ids` sort ids
  numerically and `get_custodians` is ordered by `owner_pubkey`.
- Wallets deployed from the older flat `Multisig` build still accept *writes*
  from this binding (all 17 shared functions keep their signatures and function
  ids), but decoding their storage with the v2 ABI is not valid, so the read
  methods are only defined for v2 wallets.

### Unchanged
- `submit_transaction`, `send_transaction`, `confirm_transaction` and their
  params, including `dapp_id`.
- Deploy stays out of scope — wallets are deployed by the end-user via
  `tvm-cli`. Note that on ABI ≥ 2.3 the deploy address depends on the ABI's
  `fields` list as well as the code, so a deployer must pair this TVC with this
  ABI.

## [4.0.1]

### Changed
- Bumped the `tvm-sdk` pin (`tvm_block` / `tvm_client`) from `v3.0.2.an` to
  `v3.0.4.an`.
- Added `[patch]` tables to the root `Cargo.toml`, replicated from the tvm-sdk
  workspace root: since `v3.0.4.an`, `tvm_vm` pulls the zk stack (`axiom-eth`
  plus the gosh-sh halo2 forks), and Cargo only honors `[patch]` from the
  top-level workspace — without the replicas the graph resolves two
  incompatible copies of `halo2-axiom`/`halo2-base` and fails to compile
  (E0277 `Circuit<F>` trait mismatch).

## [4.0.0]

The GraphQL server is now stable at `>= 1.0.0` across all networks, so the kit
no longer carries the legacy (`< 1.0.0`) wire format or the runtime
version-detection that switched between them. Every address-bearing query now
unconditionally uses the v3 `account(account_id, dapp_id)` form.

### Removed (breaking)
- `dapp::supports_dapp_id(context, module)` — the server-generation probe. The
  kit no longer branches on server version; the SDK already gates `dapp_id`
  internally for `get_account` / `send_message`.

### Changed
- Dropped the legacy `account(address:)` GraphQL queries and the per-call
  `if v3 { … } else { … }` branches in `event`, `authservice::root`,
  `authservice::profile`, and `accumulator` event paging. The v3 queries (their
  former `*_V3` constants, now un-suffixed) are the only form sent.

### Unchanged
- `dapp::SystemDapp` and its fixed dApp IDs.
- All public wrapper constructors and `query_*` signatures (`dapp_id` was already
  mandatory).

## [3.0.0]

DEX contract wrappers moved out of the kit into the consumer crate
(`dodex-contracts`). The kit now ships only the wrapper framework (traits +
infra); downstream crates name their module via a new open `KitModule` variant.

### Added
- `KitModule::External(&'static str)` — an open module identity for wrappers that
  live in downstream crates built on the kit traits. The payload is a stable,
  caller-chosen id (e.g. `"dex.private_note"`) and is `&'static str` so it stays
  usable in the const `ModuleAccessor::MODULE`.
- `#[non_exhaustive]` on `KitModule`, so future external module groups never
  require an enum edit.

### Removed (breaking)
- All DEX bindings and event decoders (`contracts/src/dex/`: `private_note`,
  `order_book`, `pmp`, `oracle`, `oracle_event_list`, `root_oracle`, `root_pn`,
  `nullifier`, their `*_events`, and `dex/tests.rs`) and their ABIs
  (`contracts/abi/dex/`). The `dex` module is no longer exported from the crate.
- `KitModule::Dex` and the `DexModule` enum — relocated wrappers identify their
  module via `KitModule::External("dex.<contract>")`. (The DEX market never
  shipped to mainnet, so no transition period / deprecation window was needed.)

### Unchanged (relied on downstream)
- The trait framework (`traits.rs`), `KitError`/`KitErrorCode`/`KitResult`, and
  `account`/`event`/`deserialize`/`dapp`/`giver`/`multisig` plus the `shared`
  guard traits and the root `pub use tvm_client` re-export keep their signatures.

## [2.1.0]

### Added
- `multisig` binding for the `Multisig` contract (v2 ABI): `submit_transaction`,
  `send_transaction`, `confirm_transaction`, and the read getters
  (`get_parameters`, `get_custodians`, `get_transaction(s)`,
  `get_transaction_ids`, `get_version`). Bundles the `Multisig` ABI + TVC under
  `contracts/abi/multisig/`.
- `dapp_id` (uint256) on the message-sending params — `ParamsOfSubmitTransaction`
  and `ParamsOfSendTransaction` gain a `dapp_id` field (defaults to `"0"`) — and
  on the decoded `Transaction`, matching the v2 ABI's destination dApp id.
- `KitModule::Multisig` error-module variant.
- ABI-cross-check unit tests for the multisig params: every `submitTransaction` /
  `sendTransaction` ABI input must have a matching serialized key, so a
  binding/ABI drift (e.g. `flag` vs `flags`, `dapp_id`) fails at `cargo test`
  rather than on-chain.

### Changed
- `Multisig::new` takes `impl Into<ParamsOfNewContract>` (address + dApp id),
  consistent with the other contract bindings; a user-deployed wallet is
  addressed under its own account-id dApp rather than the System dApp.
- Synced with `dev`.

## [2.0.1], [2.0.0], [1.0.0]

Predate this changelog — see the git tags and history.
