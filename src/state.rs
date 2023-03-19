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
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfo {
    pub user: String,
    pub account_type: Option<String>,
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OTCInfo {
    pub seller: CanonicalAddr,
    pub sell_native: bool,
    pub sell_amount: Uint128,
    pub sell_denom: Option<String>,
    pub sell_address: Option<Addr>,
    pub ask_native: bool,
    pub ask_amount: Uint128,
    pub ask_denom: Option<String>,
    pub ask_address: Option<Addr>,
    pub expires: Expiration,
    pub user_info: Option<UserInfo>,
    pub description: Option<String>,
}


pub const STATE: Item<State> = Item::new("state");
pub const OTCS: Map<u32, OTCInfo> = Map::new("otcs");