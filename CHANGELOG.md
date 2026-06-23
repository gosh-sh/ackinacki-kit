# Changelog

All notable changes to `ackinacki-kit` are documented here. The format loosely
follows [Keep a Changelog](https://keepachangelog.com/); the workspace is
versioned as a whole (`package.version` in the root `Cargo.toml`).

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
