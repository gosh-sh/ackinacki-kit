use std::collections::HashMap;
use std::str::FromStr;

use num_bigint::BigInt;
use num_bigint::Sign;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;

pub fn deserialize_u8<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    serde_json::from_str::<u8>(&s).map_err(Error::custom)
}

pub fn deserialize_u16<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    serde_json::from_str::<u16>(&s).map_err(Error::custom)
}

pub fn deserialize_option_u16<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => s.parse::<u16>().map(Some).map_err(Error::custom),
        None => Ok(None),
    }
}

pub fn deserialize_option_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => s.parse::<u32>().map(Some).map_err(Error::custom),
        None => Ok(None),
    }
}

pub fn deserialize_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    serde_json::from_str::<u32>(&s).map_err(Error::custom)
}

pub fn deserialize_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    serde_json::from_str::<u64>(&s).map_err(Error::custom)
}

pub fn deserialize_option_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => s.parse::<u64>().map(Some).map_err(Error::custom),
        None => Ok(None),
    }
}

pub fn deserialize_u128<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    serde_json::from_str::<u128>(&s).map_err(Error::custom)
}

pub fn deserialize_option_u128<'de, D>(deserializer: D) -> Result<Option<u128>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => s.parse::<u128>().map(Some).map_err(Error::custom),
        None => Ok(None),
    }
}

pub fn deserialize_u128_map<'de, D>(deserializer: D) -> Result<HashMap<String, u128>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_map: HashMap<String, String> = HashMap::deserialize(deserializer)?;
    let mut result = HashMap::with_capacity(raw_map.len());

    for (k, v) in raw_map {
        let parsed = v.parse::<u128>().map_err(Error::custom)?;
        result.insert(k, parsed);
    }

    Ok(result)
}

pub fn deserialize_u32_u8_u128_nested_map<'de, D>(
    deserializer: D,
) -> Result<HashMap<u32, HashMap<u8, u128>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_outer: HashMap<String, HashMap<String, String>> = HashMap::deserialize(deserializer)?;
    let mut outer = HashMap::with_capacity(raw_outer.len());

    for (outcome_id, raw_inner) in raw_outer {
        let outcome_id = outcome_id.parse::<u32>().map_err(Error::custom)?;
        let mut inner = HashMap::with_capacity(raw_inner.len());

        for (bet_type, amount) in raw_inner {
            let bet_type = bet_type.parse::<u8>().map_err(Error::custom)?;
            let amount = amount.parse::<u128>().map_err(Error::custom)?;
            inner.insert(bet_type, amount);
        }

        outer.insert(outcome_id, inner);
    }

    Ok(outer)
}

pub fn deserialize_u128_vec<'de, D>(deserializer: D) -> Result<Vec<u128>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_vec: Vec<String> = Vec::deserialize(deserializer)?;
    let mut result = Vec::with_capacity(raw_vec.len());

    for v in raw_vec {
        let parsed = v.parse::<u128>().map_err(Error::custom)?;
        result.push(parsed);
    }

    Ok(result)
}

pub fn deserialize_u64_map<'de, D>(deserializer: D) -> Result<HashMap<String, u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_map: HashMap<String, String> = HashMap::deserialize(deserializer)?;
    let mut result = HashMap::with_capacity(raw_map.len());

    for (k, v) in raw_map {
        let parsed = v.parse::<u64>().map_err(Error::custom)?;
        result.insert(k, parsed);
    }

    Ok(result)
}

pub fn deserialize_u64_vec<'de, D>(deserializer: D) -> Result<Vec<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_vec: Vec<String> = Vec::deserialize(deserializer)?;
    let mut result = Vec::with_capacity(raw_vec.len());

    for v in raw_vec {
        let parsed = v.parse::<u64>().map_err(Error::custom)?;
        result.push(parsed);
    }

    Ok(result)
}

pub fn deserialize_bigint<'de, D>(deserializer: D) -> Result<BigInt, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(BigInt::from_str(&s).unwrap())
}

pub fn deserialize_option_bigint<'de, D>(deserializer: D) -> Result<Option<BigInt>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => BigInt::from_str(&s).map(Some).map_err(Error::custom),
        None => Ok(None),
    }
}

pub fn deserialize_account_balance<'de, D>(deserializer: D) -> Result<Option<BigInt>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let bytes = {
                let s = s.trim_start_matches("0x").trim_start_matches("0X");
                let padded = format!("{:0>width$}", s, width = (s.len() + 1) & !1);
                hex::decode(padded).map_err(Error::custom)?
            };
            // Account `balance` is TVM `Grams` (`VarUInteger 16`). GraphQL
            // renders it as hexadecimal bytes, so a leading high bit is part
            // of a positive magnitude, not a two's-complement sign bit.
            Ok(Some(BigInt::from_bytes_be(Sign::Plus, &bytes)))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use serde::Deserialize;
    use serde_json::json;
    use serde_json::Value;

    use super::deserialize_account_balance;

    #[derive(Debug, Deserialize)]
    struct AccountBalanceFixture {
        #[serde(deserialize_with = "deserialize_account_balance")]
        balance: Option<BigInt>,
    }

    fn decode(value: Value) -> Result<Option<BigInt>, serde_json::Error> {
        serde_json::from_value::<AccountBalanceFixture>(json!({ "balance": value }))
            .map(|fixture| fixture.balance)
    }

    #[test]
    fn account_balance_hex_is_unsigned_at_high_bit_boundaries() {
        for (raw, expected) in [
            ("0x0", 0_u64),
            ("0x7f", 127),
            ("0x80", 128),
            ("ff", 255),
            ("0x0100", 256),
            ("0x7fffffff", 2_147_483_647),
            ("0x80000000", 2_147_483_648),
            ("0xffffffff", 4_294_967_295),
        ] {
            assert_eq!(decode(json!(raw)).unwrap(), Some(BigInt::from(expected)), "raw={raw}");
        }
    }

    #[test]
    fn account_balance_decodes_shellnet_agent_wallet_vectors() {
        // Captured from two v2.4 multisigs on shellnet (2026-08-11). The old
        // signed decoder reported -1_583_123_296 and -1_476_594_296.
        assert_eq!(decode(json!("0xa1a374a0")).unwrap(), Some(BigInt::from(2_711_844_000_u64)));
        assert_eq!(decode(json!("0xa7fcf588")).unwrap(), Some(BigInt::from(2_818_373_000_u64)));
    }

    #[test]
    fn account_balance_preserves_none_and_rejects_malformed_hex() {
        assert_eq!(decode(Value::Null).unwrap(), None);
        assert!(decode(json!("0xnot-hex")).is_err());
    }
}
