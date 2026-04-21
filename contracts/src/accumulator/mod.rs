//! Accumulator contract wrappers.
//!
//! Mirrors contracts from `acki-nacki/contracts/accumulator`:
//! - `ShellAccumulatorRootUSDC`
//! - `ShellSellOrderLot`
//!
//! This module also exposes protocol constants and amount helpers so SDK
//! consumers do not need to hardcode denomination math in app code.

pub mod events;
pub mod shell_accumulator_root_usdc;
pub mod shell_sell_order_lot;

/// 1 Shell = `1_000_000_000` nanoShell.
pub const SHELL_DECIMALS_FACTOR: u128 = 1_000_000_000;
/// 1 USDC = `1_000_000` micro-USDC.
pub const USDC_DECIMALS_FACTOR: u128 = 1_000_000;
/// Fixed protocol rate: `100 Shell` per `1 USDC` in nanoShell units.
pub const SHELL_PER_USDC: u128 = 100 * SHELL_DECIMALS_FACTOR;

pub const DENOM_1: u16 = 1;
pub const DENOM_10: u16 = 10;
pub const DENOM_100: u16 = 100;
pub const DENOM_1000: u16 = 1000;
pub const VALID_DENOMS: [u16; 4] = [DENOM_1, DENOM_10, DENOM_100, DENOM_1000];

pub const NACKL_ECC_ID: u32 = 1;
pub const SHELL_ECC_ID: u32 = 2;
pub const USDC_ECC_ID: u32 = 3;

/// Returns `true` when denomination is one of accumulator-supported values.
pub const fn is_valid_denom(d: u16) -> bool {
    matches!(d, DENOM_1 | DENOM_10 | DENOM_100 | DENOM_1000)
}

/// Converts denomination `D` (USDC units) to seller Shell deposit amount
/// in nanoShell: `D * SHELL_PER_USDC`.
pub fn shell_amount_for_denom(d: u16) -> Option<u128> {
    if !is_valid_denom(d) {
        return None;
    }
    Some(u128::from(d) * SHELL_PER_USDC)
}

/// Converts denomination `D` (USDC units) to micro-USDC amount:
/// `D * USDC_DECIMALS_FACTOR`.
pub fn usdc_amount_for_denom(d: u16) -> Option<u128> {
    if !is_valid_denom(d) {
        return None;
    }
    Some(u128::from(d) * USDC_DECIMALS_FACTOR)
}

/// Alias for claim payout amount in micro-USDC.
/// Current accumulator payout formula is identical to `usdc_amount_for_denom`.
pub fn usdc_payout_for_denom(d: u16) -> Option<u128> {
    usdc_amount_for_denom(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_helpers_match_protocol_formulas() {
        assert_eq!(shell_amount_for_denom(DENOM_1), Some(SHELL_PER_USDC));
        assert_eq!(shell_amount_for_denom(DENOM_10), Some(10 * SHELL_PER_USDC));
        assert_eq!(shell_amount_for_denom(DENOM_100), Some(100 * SHELL_PER_USDC));
        assert_eq!(shell_amount_for_denom(DENOM_1000), Some(1000 * SHELL_PER_USDC));

        assert_eq!(usdc_amount_for_denom(DENOM_1), Some(USDC_DECIMALS_FACTOR));
        assert_eq!(usdc_amount_for_denom(DENOM_10), Some(10 * USDC_DECIMALS_FACTOR));
        assert_eq!(usdc_amount_for_denom(DENOM_100), Some(100 * USDC_DECIMALS_FACTOR));
        assert_eq!(usdc_amount_for_denom(DENOM_1000), Some(1000 * USDC_DECIMALS_FACTOR));

        assert_eq!(usdc_payout_for_denom(DENOM_100), usdc_amount_for_denom(DENOM_100));
    }

    #[test]
    fn invalid_denom_returns_none() {
        assert!(!is_valid_denom(7));
        assert_eq!(shell_amount_for_denom(7), None);
        assert_eq!(usdc_amount_for_denom(7), None);
        assert_eq!(usdc_payout_for_denom(7), None);
    }
}
