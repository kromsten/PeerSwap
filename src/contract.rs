#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, Addr, WasmMsg, from_binary, BankMsg, CosmosMsg, Coin, Order,
};
use cw2::set_contract_version;

use cw20::{Balance, Cw20ReceiveMsg, Cw20CoinVerified, Cw20ExecuteMsg};
use cw_storage_plus::Bound;
use cw_utils::Expiration;

use crate::error::ContractError;
use crate::state::{State, STATE, OTCS, OTCInfo, UserInfo};
use crate::msg::{InstantiateMsg, QueryMsg, ExecuteMsg, ReceiveMsg, GetOTCsResponse, NewOTCResponse};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:otc";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 60;

macro_rules! cast {
    ($target: expr, $pat: path) => {
        {
            if let $pat(a) = $target { // #1
                a
            } else {
                panic!(
                    "mismatch variant when cast to {}", 
                    stringify!($pat)); // #2
            }
        }
    };
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = State { 
        active: true,
        index: 0,
        admin: deps.api.addr_canonicalize(info.sender.as_str())?
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}




#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    
    match msg {
        ExecuteMsg::Create(msg) => try_create_otc(
            deps,
            env,
            &info.sender,
            Balance::from(info.funds), 
            msg.ask_balance,    
            msg.expires,
            msg.user_info,
            msg.description
        ),

        ExecuteMsg::Swap { otc_id } => try_swap(
            deps, 
            &info.sender, 
            otc_id,
            Balance::from(info.funds),
            true
        ),
        
        ExecuteMsg::Receive(msg) => {
            execute_receive(deps, env, info, msg)
        }
    }
}


pub fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let msg : ReceiveMsg = from_binary(&wrapper.msg)?;

    let balance = Balance::Cw20(Cw20CoinVerified {
        address: info.sender,
        amount: wrapper.amount,
    });

    let api = deps.api;

    match msg {
        ReceiveMsg::Create(msg) => { 
            try_create_otc(
                deps, 
                env,
                &api.addr_validate(&wrapper.sender)?,
                balance,
                msg.ask_balance, 
                msg.expires,
                msg.user_info,
                msg.description
            )
        }
        ReceiveMsg::Swap { otc_id } => {
            try_swap(
                deps, 
                &api.addr_validate(&wrapper.sender)?, 
                otc_id,
                balance,
                false
            )
        }
    }


    
}


pub fn try_create_otc(
    deps: DepsMut,
    env: Env,
    seller: &Addr,
    sell_balance: Balance,
    ask_balance: Balance,
    expires: Option<Expiration>,
    user_info: Option<UserInfo>,
    description: Option<String>,
    ) -> Result<Response, ContractError> {
    

    let mut config = STATE.load(deps.storage)?;


    if !config.active {
        return Err(ContractError::Std(
            StdError::GenericErr { 
                msg: "The factory has been stopped.  No new otc can be created".to_string() 
            }
        ));
    }

    let expires = expires.unwrap_or_default();
    if expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    let mut new_otc = OTCInfo {
        seller: deps.api.addr_canonicalize(seller.as_str())?,
        sell_native: false,
        sell_amount: Uint128::from(0 as u8),
        sell_denom: None,
        sell_address: None,
        ask_native: false,
        ask_amount: Uint128::from(0 as u8),
        ask_denom: None,
        ask_address: None,
        expires,
        user_info,
        description
    };


    match sell_balance {
        Balance::Native(mut balance) => {
            let coin = balance.0.pop().unwrap();


            if balance.0.len() != 0 {
                return Err(ContractError::Std(
                    StdError::GenericErr { 
                        msg: "Cannot create an otc with mupltiple denoms".to_string() 
                    }
                ));
            }
            new_otc.sell_native = true;
            new_otc.sell_amount = coin.amount;
            new_otc.sell_denom = Some(coin.denom);
        },
        Balance::Cw20(token) => {
            new_otc.sell_native = false;
            new_otc.sell_amount = token.amount;
            new_otc.sell_address = Some(token.address);
        }
    };


    match ask_balance {
        Balance::Native(mut balance) => {

            let coin = balance.0.pop().unwrap();

            if balance.0.len() != 0 { return Err(ContractError::TooManyDenoms{}); }

            new_otc.ask_native = true;
            new_otc.ask_amount = coin.amount;
            new_otc.ask_denom = Some(coin.denom);
        },
        Balance::Cw20(token) => {
            new_otc.ask_native = false;
            new_otc.ask_amount = token.amount;
            new_otc.ask_address = Some(token.address);
        }
    };


    while OTCS.has(deps.storage, config.index) {
        // okay for ~4 billion
        config.index += 1;    
    }

    OTCS.save(deps.storage, config.index, &new_otc)?;
    STATE.save(deps.storage, &config)?; 
   

    let data = NewOTCResponse {
        id: config.index,
        otc: new_otc
    };

    Ok(Response::new()
        .set_data(to_binary(&data).unwrap())
        .add_attribute("method", "create_new_otc")
    )

}



pub fn try_swap(
    deps: DepsMut,
    payer: &Addr,
    otc_id: u32,
    balance: Balance,
    native: bool,
    ) -> Result<Response, ContractError> {
    
    let otc_info = OTCS.load(deps.storage, otc_id)?;

    if otc_info.ask_native ^ native { return Err(ContractError::WrongDenom {}); }

    let seller = deps.api.addr_humanize(&otc_info.seller)?;

    if &seller == payer {
        return Err(ContractError::Std(
            StdError::GenericErr { 
                msg: "Can't swap with yourself".to_string() 
            }
        ));
    }


    let payment_1 : CosmosMsg = if native {
        let mut casted =  cast!(balance, Balance::Native);
        let coin = casted.0.pop().unwrap();

        if casted.0.len() != 0 { return Err(ContractError::TooManyDenoms{}); }
        if coin.denom != otc_info.ask_denom.unwrap() { return Err(ContractError::WrongDenom {}); }

        if coin.amount < otc_info.ask_amount {
            return Err(ContractError::Std(
                StdError::GenericErr { 
                    msg: "Sent amount is smaller than what being asked".to_string() 
                }
            ));
        }

        CosmosMsg::Bank(BankMsg::Send { to_address: seller.into_string(), amount: vec!(coin) })
        

    } else {
        let casted = cast!(balance, Balance::Cw20);

        if casted.address != otc_info.ask_address.unwrap() { return Err(ContractError::WrongDenom {}); }

        if casted.amount < otc_info.ask_amount {
            return Err(ContractError::Std(
                StdError::GenericErr { 
                    msg: "Send amount is smaller than what being asked".to_string() 
                }
            ));
        }

        CosmosMsg::Wasm(WasmMsg::Execute { 
            contract_addr: casted.address.to_string(), 
            msg: to_binary(&Cw20ExecuteMsg::Transfer { recipient: seller.to_string(), amount: casted.amount })?, 
            funds: vec!()
        })
        
    };


    let payment_2 : CosmosMsg = if otc_info.sell_native {
        CosmosMsg::Bank(BankMsg::Send { 
            to_address: payer.clone().into_string(), 
            amount: vec!(Coin { denom: otc_info.sell_denom.unwrap(), amount: otc_info.sell_amount }) 
        })
    } else {
        CosmosMsg::Wasm(WasmMsg::Execute { 
            contract_addr: otc_info.sell_address.unwrap().to_string(), 
            msg: to_binary(&Cw20ExecuteMsg::Transfer { 
                recipient: payer.to_string(), 
                amount: otc_info.sell_amount 
            })?, 
            funds: vec!()
        })
    };


    OTCS.remove(deps.storage, otc_id);
    

    Ok(Response::new()
        .add_messages(vec!(
            payment_1,
            payment_2
        ))
        .add_attribute("method", "swap")
    )
}



#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetOtcs {
            include_expired, 
            start_after, 
            limit 
        } =>to_binary(&query_otcs(
            deps, 
            env, 
            include_expired.unwrap_or_default(),
            start_after,
            limit
        )?)
    }
}



fn query_otcs(
    deps: Deps, 
    env: Env, 
    include_expired: bool,
    start_after: Option<u32>,
    limit: Option<u32>,
) -> StdResult<GetOTCsResponse> {

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    
    let start = match start_after {
        Some(start_after) => Some(Bound::exclusive(start_after)),
        None => None
    };

    let result : StdResult<Vec<_>> = OTCS
    .range(
        deps.storage, 
        start, 
        None, 
        Order::Ascending
    )
    .filter(|otc| 
        include_expired || (
            otc.is_ok() && 
            !otc.as_ref().unwrap().1.expires.is_expired(&env.block) 
        )
    )
    .take(limit)
    .collect();

    //OTCS.load(deps.storage, )
    Ok(GetOTCsResponse { otcs: result? })
}




