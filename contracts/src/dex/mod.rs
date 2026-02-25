//! DEX contract wrappers and typed event decoders.
//!
//! The modules in this namespace mirror the Solidity contracts from
//! `acki-nacki/contracts/dex` and follow the migrated wrapper style:
//! `ContractBase + HasContractBase + AutoContract`.

pub mod nullifier;
pub mod oracle;
pub mod oracle_event_list;
pub mod oracle_event_list_events;
pub mod oracle_events;
pub mod pmp;
pub mod pmp_events;
pub mod private_note;
pub mod private_note_events;
pub mod root_oracle;
pub mod root_oracle_events;
pub mod root_pn;
pub mod root_pn_events;

#[cfg(test)]
mod tests;
