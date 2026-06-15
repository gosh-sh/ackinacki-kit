# Changelog

All notable changes to `ackinacki-kit` are documented here. The format loosely
follows [Keep a Changelog](https://keepachangelog.com/); the workspace is
versioned as a whole (`package.version` in the root `Cargo.toml`).

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
