#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, Addr, WasmMsg, from_binary, BankMsg, CosmosMsg, Coin, Order, Decimal,
};
use cw2::set_contract_version;

use cw20::{Balance, Cw20ReceiveMsg, Cw20CoinVerified, Cw20ExecuteMsg};
use cw_storage_plus::Bound;
use cw_utils::Expiration;

use crate::error::ContractError;
use crate::state::{State, STATE, OTCS, OTCInfo, UserInfo, AskFor};
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
            msg.ask_balances,    
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
                msg.ask_balances, 
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



pub fn try_cancell_otc(
    deps: DepsMut,
    env: Env,
    sender: &Addr,
    otc_id: u32,
    ) -> Result<Response, ContractError> {

    let res = OTCS.load(deps.storage, otc_id);
        
    // unwrap or return error
    if !res.is_ok() { return  Err(ContractError::NotFound {  }) }; 

    let otc = res.unwrap();

    if otc.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    };

    let seller = deps.api.addr_humanize(&otc.seller)?;
    if sender != &seller {
        return Err(ContractError::Unauthorized {});
    };


    let payment = if otc.sell_native {
        CosmosMsg::Bank(BankMsg::Send {
            to_address: seller.clone().to_string(),
            amount: vec![Coin {
                denom: env.contract.address.to_string(),
                amount: otc.sell_amount.into(),
            }],
        })
    } else {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: otc.sell_address.clone().unwrap().to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: deps.api.addr_humanize(&otc.seller)?.to_string(),
                amount: otc.sell_amount.into(),
            })?,
        })
    };

    OTCS.remove(deps.storage, otc_id);

    Ok(Response::new()
        .add_messages(vec!(
            payment
        ))
        .add_attribute("method", "cancell")
        .add_attributes(vec![
            ("otc_id", otc_id.to_string()),
            ("amount", otc.sell_amount.to_string()),
            ("currency", if otc.sell_native { otc.sell_denom.unwrap() } else { String::from("cw20:") + &otc.sell_address.unwrap().to_string() }),
        ])
    )
}


pub fn try_create_otc(
    deps: DepsMut,
    env: Env,
    seller: &Addr,
    sell_balance: Balance,
    ask_balances: Vec<Balance>,
    expires: Option<Expiration>,
    user_info: Option<UserInfo>,
    description: Option<String>,
    ) -> Result<Response, ContractError> {
    

    let mut config = STATE.load(deps.storage)?;


    if !config.active {
        return Err(ContractError::Std(
            StdError::GenericErr { 
                msg: "The factory has been stopped. No new otc can be created".to_string() 
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
        initial_sell_amount: Uint128::from(0 as u8),
        sell_denom: None,
        sell_address: None,
        ask_for: vec![],
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
            new_otc.initial_sell_amount = coin.amount;
            new_otc.sell_denom = Some(coin.denom);
        },
        Balance::Cw20(token) => {
            new_otc.sell_native = false;
            new_otc.sell_amount = token.amount;
            new_otc.initial_sell_amount = token.amount;
            new_otc.sell_address = Some(token.address);
        }
    };

    for ask_balance in ask_balances {
        match ask_balance {
            Balance::Native(balance) => {


                for coin in balance.0 {
                    new_otc.ask_for.push(AskFor {
                        native: true,
                        amount: coin.amount,
                        initial_amount: coin.amount,
                        denom: Some(coin.denom),
                        address: None
                    });
                }

            },


            Balance::Cw20(token) => {
                new_otc.ask_for.push(AskFor {
                    native: false,
                    amount: token.amount,
                    initial_amount: token.amount,
                    denom: None,
                    address: Some(token.address)
                })
            }
        };
    }

 

    while OTCS.has(deps.storage, config.index) {
        // okay for ~4 billion
        config.index += 1;    
    }

    OTCS.save(deps.storage, config.index, &new_otc)?;
    STATE.save(deps.storage, &config)?; 
   

    let data = NewOTCResponse {
        id: config.index,
        otc: new_otc.clone()
    };


    Ok(Response::new()
        .set_data(to_binary(&data).unwrap())
        .add_attributes(vec![
            ("method", "create_new_otc"),
            ("otc_id", &config.index.to_string()),
            ("seller", &seller.to_string()),
            ("amount", &new_otc.sell_amount.to_string()),
            ("currency", &new_otc.sell_denom.unwrap_or_else(|| String::from("cw20:") + &new_otc.sell_address.unwrap().to_string())),
        ])
    )
}



pub fn try_swap(
    deps: DepsMut,
    payer: &Addr,
    otc_id: u32,
    balance: Balance,
    native: bool,
    ) -> Result<Response, ContractError> {
    
    let mut otc_info = OTCS.load(deps.storage, otc_id)?;


    let seller = deps.api.addr_humanize(&otc_info.seller)?;

    if &seller == payer {
        return Err(ContractError::Std(
            StdError::GenericErr { 
                msg: "Can't swap with yourself".to_string() 
            }
        ));
    }
   
   let to_sell_amount : Uint128;
   let swapped_amount : Uint128; 
   let swapped_currency : String;

    let payment_1 : CosmosMsg = if native {

        let mut casted =  cast!(balance, Balance::Native);
        let coin = casted.0.pop().unwrap();

        if casted.0.len() != 0 { return Err(ContractError::TooManyDenoms{}); }


        let to_pay = otc_info.ask_for
            .iter()
            .find(|ask| matches!(ask.native, true) && ask.denom.as_ref() == Some(&coin.denom))
            .and_then(|ask| Some(ask))
            .ok_or(ContractError::WrongDenom {})?;


        let ratio = if to_pay.amount  > coin.amount  {
                Decimal::from_ratio(to_pay.amount - coin.amount, to_pay.amount)
            }
            else {
                Decimal::one()
        };
        
        to_sell_amount =  otc_info.sell_amount - otc_info.sell_amount * ratio;
        swapped_amount = coin.amount.clone();
        swapped_currency = coin.denom.clone();


        otc_info.ask_for = otc_info.ask_for
            .iter()
            .map(|ask|  AskFor { amount: ask.amount * ratio, ..ask.clone() }
            )
            .collect();

        CosmosMsg::Bank(BankMsg::Send { to_address: seller.into_string(), amount: vec!(coin) })
        

    } else {
        let casted = cast!(balance, Balance::Cw20);

        let to_pay = otc_info.ask_for
            .iter()
            .find(|ask| matches!(ask.native, false) && ask.address.as_ref() == Some(&casted.address))
            .and_then(|ask| Some(ask))
            .ok_or(ContractError::WrongDenom {})?;




        let ratio = if to_pay.amount  > casted.amount  {
            Decimal::from_ratio(to_pay.amount - casted.amount, to_pay.amount)
        }
        else {
            Decimal::one()
        };

        to_sell_amount =  otc_info.sell_amount -otc_info.sell_amount * ratio;

        swapped_amount = casted.amount;
        swapped_currency = casted.address.to_string();

        otc_info.ask_for = otc_info.ask_for
            .iter()
            .map(|ask|  AskFor { amount: ask.amount * ratio, ..ask.clone() }
            )
            .collect();


        CosmosMsg::Wasm(WasmMsg::Execute { 
            contract_addr: casted.address.to_string(), 
            msg: to_binary(&Cw20ExecuteMsg::Transfer { recipient: seller.to_string(), amount: casted.amount })?, 
            funds: vec!()
        })
        
    };



    let payment_2 : CosmosMsg = if otc_info.sell_native {
        CosmosMsg::Bank(BankMsg::Send { 
            to_address: payer.clone().into_string(), 
            amount: vec!(Coin { denom: otc_info.sell_denom.clone().unwrap(), amount: to_sell_amount }) 
        })
    } else {
        CosmosMsg::Wasm(WasmMsg::Execute { 
            contract_addr: otc_info.sell_address.clone().unwrap().to_string(), 
            msg: to_binary(&Cw20ExecuteMsg::Transfer { 
                recipient: payer.to_string(), 
                amount: to_sell_amount 
            })?, 
            funds: vec!()
        })
    };  


    if otc_info.sell_amount <= Uint128::zero() {
        OTCS.remove(deps.storage, otc_id);
    } else {
        OTCS.save(deps.storage, otc_id, &otc_info)?;
    }

    

    Ok(Response::new()
        .add_messages(vec!(
            payment_1,
            payment_2
        ))
        .add_attribute("method", "swap")
        .add_attributes(vec![
            ("otc_id", otc_id.to_string()),
            ("given amount", to_sell_amount.to_string()),
            ("given_currency", if otc_info.sell_native { otc_info.sell_denom.unwrap() } else { otc_info.sell_address.unwrap().to_string() }),
            ("sent_amount", swapped_amount.to_string()),
            ("sent_currency", swapped_currency)
        ])
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




