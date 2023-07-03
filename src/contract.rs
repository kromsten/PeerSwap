#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    Deps, DepsMut, Env, Response, StdResult, Event, Attribute, Addr,
    MessageInfo, WasmMsg, BankMsg, CosmosMsg,
    Coin, Order, Decimal, Uint128,
    Binary, to_binary, from_binary
};
use cw2::set_contract_version;

use cw20::{Balance, Cw20ReceiveMsg, Cw20CoinVerified, Cw20ExecuteMsg};
use cw_storage_plus::Bound;
use cw_utils::Expiration;

use crate::error::ContractError;
use crate::state::{State, STATE, OTCS, OTCInfo, UserInfo, AskFor};
use crate::msg::{InstantiateMsg, QueryMsg, ExecuteMsg, ReceiveMsg, GetOTCsResponse, NewOTCResponse, GetConfigResponse};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:peerswap";
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
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = State { 
        active: true,
        index: 0,
        admin: deps.api.addr_canonicalize(info.sender.as_str())?,
        taker_fee: msg.taker_fee.unwrap_or(2u16),
        maker_fee: msg.maker_fee.unwrap_or(1u16),
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
            env,
            &info.sender, 
            otc_id,
            Balance::from(info.funds),
            true
        ),

        ExecuteMsg::Cancel { otc_id } => try_cancel_otc(
            deps, 
            env, 
            &info.sender, 
            otc_id
        ),

        ExecuteMsg::SetActive { active } => try_set_active(
            deps, 
            &info.sender, 
            active
        ),

        ExecuteMsg::RemoveExpired {} => remove_expired(
            deps, 
            env
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
                env,
                &api.addr_validate(&wrapper.sender)?, 
                otc_id,
                balance,
                false
            )
        }
    }
    
}


pub fn try_set_active(
    deps: DepsMut,
    sender: &Addr,
    active: bool
) -> Result<Response, ContractError>  {

    let mut state : State = STATE.load(deps.storage)?;

    if deps.api.addr_canonicalize(sender.as_str())? != state.admin {
        return Err(ContractError::Unauthorized {});
    }

    state.active = active;

    STATE.save(deps.storage, &state)?;

    return Ok(Response::new()
        .add_attribute("method", "set_active")
        .add_attribute("active", active.to_string())
    );
}


pub fn refund_payment(
    deps: Deps,
    _env: Env,
    otc: &OTCInfo,
    seller: &Addr
) -> CosmosMsg {
    
    let payment = if otc.sell_native {
        CosmosMsg::Bank(BankMsg::Send {
            to_address: seller.clone().to_string(),
            amount: vec![Coin {
                denom: otc.sell_denom.clone().unwrap().to_string(),
                amount: otc.sell_amount.into(),
            }],
        })
    } else {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: otc.sell_address.clone().unwrap().to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: deps.api.addr_humanize(&otc.seller).unwrap().to_string(),
                amount: otc.sell_amount.into(),
            }).unwrap(),
        })
    };

    payment
}



pub fn try_cancel_otc(
    deps: DepsMut,
    env: Env,
    sender: &Addr,
    otc_id: u32,
    ) -> Result<Response, ContractError> {

    let res = OTCS.load(deps.storage, otc_id);
    if !res.is_ok() { return  Err(ContractError::NotFound {  }) }; 

    let otc = res.unwrap();

    let seller = deps.api.addr_humanize(&otc.seller)?;
    if sender != &seller {
        return Err(ContractError::Unauthorized {});
    };

    let payment = refund_payment(deps.as_ref(), env, &otc, &seller);

    OTCS.remove(deps.storage, otc_id);

    Ok(Response::new()
        .add_messages(vec!(
            payment
        ))
        .add_event(
            Event::new("peerswap_cancel")
            .add_attributes(vec![
                ("otc_id", otc_id.to_string()),
                ("amount", otc.sell_amount.to_string()),
                ("token", if otc.sell_native { 
                    otc.sell_denom.unwrap() 
                } else { 
                    String::from("cw20:") + &otc.sell_address.unwrap().to_string() 
                }),
                ("method", "cancel".to_string())
            ])

        )
    )
}



pub fn remove_expired(
    deps: DepsMut,
    env: Env
) -> Result<Response, ContractError> {

    let result : StdResult<Vec<_>> = OTCS
    .range(
        deps.storage, 
        None, 
        None, 
        Order::Ascending
    )
    .filter(|otc|  
        otc.is_ok() && 
        otc.as_ref().unwrap().1.expires.is_expired(&env.block) 
    )
    .collect();

    let expired_otcs = result.unwrap();


    let mut logs = vec![
        ("method", String::from("remove_expired")),
    ];

    for (id, otc) in expired_otcs {
        
        refund_payment(deps.as_ref(), env.clone(), &otc, &deps.api.addr_humanize(&otc.seller).unwrap());
        
        OTCS.remove(deps.storage, id);
        
        let log_text = format!("{} : {} {} to {}", 
                id, 
                otc.sell_amount, 
                if otc.sell_native { otc.sell_denom.unwrap() } else { String::from("cw20:") + &otc.sell_address.unwrap().to_string() }, 
                otc.seller
        );

        logs.push(("refunded", log_text));
    }


    Ok(Response::new()
        .add_event(
            Event::new("peerswap_remove_expired")
            .add_attributes(logs)
        )
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
        return Err(ContractError::Stopped {});
    }

    let expires = expires.unwrap_or_default();
    if expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    let mut new_otc = OTCInfo {
        seller: deps.api.addr_canonicalize(seller.as_str())?,
        expires,
        user_info,
        description,
        // default feilds
        sell_native: true,
        sell_amount: Uint128::zero(),
        initial_sell_amount: Uint128::zero(),
        sell_denom: None,
        sell_address: None,
        ask_for: vec![],
    };


    if ask_balances.len() == 0 {
        return Err(ContractError::NoAskTokens {});
    }

    
    match sell_balance {
        Balance::Native(mut balance) => {
            let coin = balance.0.pop().unwrap();


            if balance.0.len() != 0 {
                return Err(ContractError::TooManyGiveTokens {});
            }

            if coin.amount < Uint128::from(10000u128) {
                return Err(ContractError::TooSmall {});
            }

            new_otc.sell_native = true;
            new_otc.sell_amount = coin.amount;
            new_otc.initial_sell_amount = coin.amount;
            new_otc.sell_denom = Some(coin.denom);
        },
        Balance::Cw20(token) => {

            if token.amount < Uint128::from(10000u128) {
                return Err(ContractError::TooSmall {});
            }

            new_otc.sell_native = false;
            new_otc.sell_amount = token.amount;
            new_otc.initial_sell_amount = token.amount;
            new_otc.sell_address = Some(token.address);
        }
    };

    

    for ask_balance in ask_balances {
        match ask_balance {
            Balance::Native(balance) => {

                if balance.0.len() == 0 {
                    return Err(ContractError::NoAskTokens {});
                }

                for coin in balance.0 {

                    if new_otc.sell_native && new_otc.sell_denom.clone().unwrap() == coin.denom {
                        return Err(ContractError::SameToken {});
                    } 

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

                if !new_otc.sell_native && new_otc.sell_address.clone().unwrap() == token.address {
                    return Err(ContractError::SameToken {});
                }

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
        // rotate around ~4 billion
        config.index = config.index + 1 % u32::MAX;    
    }

    OTCS.save(deps.storage, config.index, &new_otc)?;
    STATE.save(deps.storage, &config)?; 
   
    
    let data = NewOTCResponse {
        id: config.index,
        otc: new_otc.clone()
    };

    Ok(Response::new()
        .set_data(to_binary(&data).unwrap())
        .add_event(
            Event::new("peerswap_otc_created")
            .add_attributes(vec![
                ("otc_id", &config.index.to_string()),
                ("seller", &seller.to_string()),
                ("amount", &new_otc.sell_amount.to_string()),
                ("token", &new_otc.sell_denom.unwrap_or_else(|| String::from("cw20:") + &new_otc.sell_address.unwrap().to_string())),
                ("method", &"create_otc".to_string())
            ])
        )
    )
}



pub fn try_swap(
    deps: DepsMut,
    env: Env,
    payer: &Addr,
    otc_id: u32,
    balance: Balance,
    native: bool,
    ) -> Result<Response, ContractError> {



    let config = STATE.load(deps.storage)?;
    let mut otc_info = OTCS.load(deps.storage, otc_id)?;

    let seller = deps.api.addr_humanize(&otc_info.seller)?;


    let expires = otc_info.expires;
    if expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }
   
   let to_sell_amount : Uint128;
   let swapped_amount : Uint128; 
   let swapped_token : String;

   let mut payments : Vec<CosmosMsg> = Vec::with_capacity(4);

   let admin = deps.api.addr_humanize(&config.admin)?.to_string();


   if native {

        let mut casted =  cast!(balance, Balance::Native);

        if casted.0.len() == 0 { return Err(ContractError::WrongDenom {} ); }

        let coin = casted.0.pop().unwrap();

        if casted.0.len() != 0 { return Err(ContractError::TooManyDenoms{}); }

        if coin.amount != otc_info.sell_amount && coin.amount < Uint128::from(10000u128) {
            return Err(ContractError::TooSmall {});
        }

        let to_pay = otc_info.ask_for
            .iter()
            .find(|ask| matches!(ask.native, true) && ask.denom.as_ref() == Some(&coin.denom))
            .and_then(|ask| Some(ask))
            .ok_or(ContractError::WrongDenom {})?;



        let ratio = if to_pay.amount  > coin.amount  {
                Decimal::from_ratio(to_pay.amount - coin.amount, to_pay.amount)
            }
            else {
                Decimal::zero()
        };
        
        to_sell_amount =  otc_info.sell_amount - otc_info.sell_amount * ratio;
        otc_info.sell_amount -= to_sell_amount;

        swapped_amount = coin.amount.clone();
        swapped_token = coin.denom.clone();

        otc_info.ask_for = otc_info.ask_for
            .iter()
            .map(|ask|  AskFor { amount: ask.amount * ratio, ..ask.clone() }
            )
            .collect();

        let fee = coin.amount * Decimal::from_ratio(config.taker_fee, 10000u16);

        payments.push(
            CosmosMsg::Bank(BankMsg::Send { 
                to_address: seller.clone().into_string(), 
                amount: vec!(Coin { denom: coin.denom.clone(), amount: coin.amount - fee }) })
        );

        payments.push(
            CosmosMsg::Bank(BankMsg::Send { 
                to_address: admin.clone(), 
                amount: vec!(Coin { denom: coin.denom, amount: fee }) })
        )
        

    } else {
        let casted = cast!(balance, Balance::Cw20);

        let to_pay = otc_info.ask_for
            .iter()
            .find(|ask| matches!(ask.native, false) && ask.address.as_ref() == Some(&casted.address))
            .and_then(|ask| Some(ask))
            .ok_or(ContractError::WrongDenom {})?;

    
        let ratio = if to_pay.amount  > casted.amount  {
            //Decimal::one()
            Decimal::from_ratio(to_pay.amount - casted.amount, to_pay.amount)
        }
        else {
            Decimal::zero()
        };


        if casted.amount != otc_info.sell_amount && casted.amount < Uint128::from(10000u128) {
            return Err(ContractError::TooSmall {});
        }


        to_sell_amount =  otc_info.sell_amount - otc_info.sell_amount * ratio;
        
        otc_info.sell_amount -= to_sell_amount;



        swapped_amount = casted.amount;
        swapped_token = casted.address.to_string();


        otc_info.ask_for = otc_info.ask_for
            .iter()
            .map(|ask|  AskFor { amount: ask.amount * ratio, ..ask.clone() }
            )
            .collect();

        let fee = casted.amount * Decimal::from_ratio(config.taker_fee, 10000u16);


        payments.push(
            CosmosMsg::Wasm(WasmMsg::Execute { 
                contract_addr: casted.address.to_string(), 
                msg: to_binary(&Cw20ExecuteMsg::Transfer { 
                    recipient: seller.to_string(), 
                    amount: casted.amount - fee 
                })?, 
                funds: vec!()
            })
        );

        payments.push(
            CosmosMsg::Wasm(WasmMsg::Execute { 
                contract_addr: casted.address.to_string(), 
                msg: to_binary(&Cw20ExecuteMsg::Transfer { 
                    recipient: admin.clone(),
                    amount: fee 
                })?, 
                funds: vec!()
            })
        )
        
    };


    let fee = to_sell_amount * Decimal::from_ratio(config.maker_fee, 10000u16);


    if otc_info.sell_native {

        payments.push(
            CosmosMsg::Bank(BankMsg::Send { 
                to_address: payer.clone().into_string(), 
                amount: vec!(Coin { 
                    denom: otc_info.sell_denom.clone().unwrap(), 
                    amount: to_sell_amount - fee
                }) 
            })
        );

        payments.push(
            CosmosMsg::Bank(BankMsg::Send { 
                to_address: admin, 
                amount: vec!(Coin { 
                    denom: otc_info.sell_denom.clone().unwrap(), 
                    amount: fee
                }) 
            })
        )

    } else {


        payments.push(
            CosmosMsg::Wasm(WasmMsg::Execute { 
                contract_addr: otc_info.sell_address.clone().unwrap().to_string(), 
                msg: to_binary(&Cw20ExecuteMsg::Transfer { 
                    recipient: payer.to_string(), 
                    amount: to_sell_amount - fee
                })?, 
                funds: vec!()
            })
        );
        

        payments.push(
            CosmosMsg::Wasm(WasmMsg::Execute { 
                contract_addr: otc_info.sell_address.clone().unwrap().to_string(), 
                msg: to_binary(&Cw20ExecuteMsg::Transfer { 
                    recipient: admin, 
                    amount: fee 
                })?, 
                funds: vec!()
            })
        )
    };  

    let attributes: Vec<Attribute> = vec![
        Attribute {
            key: String::from("seller"),
            value: seller.to_string()
        },

        Attribute {
            key: String::from("otc_id"),
            value: otc_id.to_string()
        },

        Attribute {
            key: String::from("given_amount"),
            value: to_sell_amount.to_string()
        },

        Attribute {
            key: String::from("given_token"),
            value: if otc_info.sell_native { 
                otc_info.sell_denom.clone().unwrap() 
            } else { 
                otc_info.sell_address.clone().unwrap().to_string() 
            }
        },

        Attribute {
            key: String::from("sent_amount"),
            value: swapped_amount.to_string()
        },

        Attribute {
            key: String::from("sent_token"),
            value: swapped_token
        },

        Attribute {
            key: String::from("method"),
            value: String::from("swap")
        }
    ];



    let event_type = if otc_info.sell_amount <= Uint128::zero() {
        OTCS.remove(deps.storage, otc_id);
        "peerswap_swap_completed"
    } else {
        OTCS.save(deps.storage, otc_id, &otc_info)?;
        "peerswap_swap"
    };


    Ok(Response::new()
        .add_messages(payments)
        .add_event(Event::new(event_type).add_attributes(attributes))
    )
}



#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetOtcs {
            include_expired, 
            start_after, 
            limit 
        } => to_binary(&query_otcs(
            deps, 
            env, 
            include_expired.unwrap_or_default(),
            start_after,
            limit
        )?),

        QueryMsg::GetAddressOtcs { 
            address,
            include_expired, 
            start_after, 
            limit 
         } => to_binary(&query_addr_otcs(
            deps, 
            env, 
            address,
            include_expired.unwrap_or_default(),
            start_after,
            limit
        )?),

        QueryMsg::GetOtc {
            otc_id, 
        } => to_binary(&query_otc(
            deps, 
            otc_id
        )?),

        QueryMsg::Config {} => to_binary(&query_config(deps)?),
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
        otc.is_ok() && 
        if include_expired { 
            true
        } else {
            !otc.as_ref().unwrap().1.expires.is_expired(&env.block)
        } 
    )
    .take(limit)
    .collect();

    //OTCS.load(deps.storage, )
    Ok(GetOTCsResponse { otcs: result? })
}


fn query_addr_otcs(
    deps: Deps, 
    env: Env, 
    addr: Addr,
    include_expired: bool,
    start_after: Option<u32>,
    limit: Option<u32>,
) -> StdResult<GetOTCsResponse> {

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    
    let start = match start_after {
        Some(start_after) => Some(Bound::exclusive(start_after)),
        None => None
    };

    
    let canon = deps.api.addr_canonicalize(addr.as_str())?;
    
    let result : StdResult<Vec<_>> = OTCS
    .range(
        deps.storage, 
        start, 
        None, 
        Order::Ascending
    )
    .filter(|otc| {
        if otc.is_ok() {
            let otc = &otc.as_ref().unwrap().1;
            let mut ok = otc.seller == canon;
            if ok {
                if !include_expired {
                    ok = !otc.expires.is_expired(&env.block);
             
                }
            }
            ok
        } else {
            false
        }

    })
    .take(limit)
    .collect();

    Ok(GetOTCsResponse { otcs: result? })
}



fn query_otc(
    deps: Deps, 
    otc_id: u32
) -> StdResult<OTCInfo> {
    Ok(OTCS.load(deps.storage, otc_id)?)
}




fn query_config(deps: Deps) -> StdResult<GetConfigResponse> {
    let config = STATE.load(deps.storage)?;
    Ok(GetConfigResponse {
        active: config.active,
        maker_fee: config.maker_fee,
        taker_fee: config.taker_fee,
        admin: deps.api.addr_humanize(&config.admin)?.to_string(),
    })
}


