use std::collections::HashMap;
use std::str::FromStr;

use num_bigint::BigInt;
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
        Some(s) => s.parse::<u16>().map(Some).map_err(serde::de::Error::custom),
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

pub fn deserialize_u128<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    serde_json::from_str::<u128>(&s).map_err(Error::custom)
}

pub fn deserialize_u128_map<'de, D>(deserializer: D) -> Result<HashMap<String, u128>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_map: HashMap<String, String> = HashMap::deserialize(deserializer)?;
    let mut result = HashMap::with_capacity(raw_map.len());

    for (k, v) in raw_map {
        let parsed = v.parse::<u128>().map_err(serde::de::Error::custom)?;
        result.insert(k, parsed);
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
        let parsed = v.parse::<u64>().map_err(serde::de::Error::custom)?;
        result.insert(k, parsed);
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
        Some(s) => BigInt::from_str(&s).map(Some).map_err(serde::de::Error::custom),
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
                hex::decode(padded).map_err(serde::de::Error::custom)?
            };
            Ok(Some(BigInt::from_signed_bytes_be(&bytes)))
        }
        None => Ok(None),
    }
}
