use cw_utils::Expiration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cw20::{Balance, Cw20ReceiveMsg};

use crate::state::{UserInfo, OTCInfo};



#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NewOTC {
    pub ask_balance: Balance, 

    // seconds since epoch
    pub expires: Option<Expiration>,

    // optional user info
    pub user_info: Option<UserInfo>,

    // optional description
    pub description: Option<String>
}




#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Create(NewOTC),

    Swap {
        otc_id: u32
    },

    /// This accepts a properly-encoded ReceiveMsg from a cw20 contract
    Receive(Cw20ReceiveMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveMsg {
    
    Create(NewOTC),

    Swap {
        otc_id: u32
    }
}



#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetOtcs { 
        include_expired: Option<bool>,
        start_after: Option<u32>,
        limit: Option<u32>
    },
}



// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetOTCsResponse {
    pub otcs: Vec<(u32, OTCInfo)>
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NewOTCResponse {
    pub id: u32,
    pub otc: OTCInfo,
}