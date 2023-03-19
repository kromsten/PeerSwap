/* use cosmwasm_std::StdResult;

use crate::state::OTCInfo;

pub fn parse_otcs(item: StdResult<(u8, OTCInfo)>) -> StdResult<(u32, OTCInfo)> {
    item.map(|(key, info)| {
        (u32::from_le_bytes(key),
        info)
    })
} */