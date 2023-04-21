use cw_utils::Expiration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Addr, Uint128};
use cw_storage_plus::{Item, Map};


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub admin: CanonicalAddr,
    pub index: u32,
    pub active: bool,
    pub taker_fee: u8, // 2nd decimal, e.g. 5 = 0.05%
    pub maker_fee: u8, // 2nd decimal 
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfo {
    pub user: String,
    pub account_type: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AskFor {
    pub initial_amount: Uint128,
    pub amount: Uint128,
    pub denom: Option<String>,
    pub address: Option<Addr>,
    pub native: bool,
}




#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OTCInfo {
    pub seller: CanonicalAddr,
    pub sell_native: bool,
    pub sell_amount: Uint128,
    pub initial_sell_amount: Uint128,
    pub sell_denom: Option<String>,
    pub sell_address: Option<Addr>,
    pub ask_for: Vec<AskFor>,
    pub expires: Expiration,
    pub user_info: Option<UserInfo>,
    pub description: Option<String>,
}


pub const STATE: Item<State> = Item::new("state");
pub const OTCS: Map<u32, OTCInfo> = Map::new("otcs");