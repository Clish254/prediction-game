#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Api, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{
    AllRoundsResponse, ExecuteMsg, InstantiateMsg, QueryMsg, RoundResponse,
    TreasuryPoolDenomResponse, UserBetResponse,
};
use crate::state::{
    Bet, Config, Round, RoundDenomBet, Side, TreasuryPoolDenom, BET, CONFIG, ROUND, ROUNDDENOMBET,
    TREASURYPOOLDENOM,
};
use kujira::querier::KujiraQuerier;
use kujira::query::KujiraQuery;

const CONTRACT_NAME: &str = "crates.io:prediction-game";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config {
        admins: map_validate(deps.api, &msg.admins)?,
        asset_denom: msg.asset_denom,
        accepted_bet_denoms: msg.accepted_bet_denoms,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "instantiate"))
}

pub fn map_validate(api: &dyn Api, admins: &[String]) -> StdResult<Vec<Addr>> {
    admins.iter().map(|addr| api.addr_validate(addr)).collect()
}

pub fn sender_is_admin(deps: &DepsMut<KujiraQuery>, sender: &str) -> StdResult<bool> {
    let cfg = CONFIG.load(deps.storage)?;
    let can = cfg.is_admin(&sender);
    Ok(can)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateAdmins { admins } => execute_update_admins(deps, info, admins),
        ExecuteMsg::UpdateAssetDenom { asset_denom } => {
            execute_update_asset_denom(deps, info, asset_denom)
        }
        ExecuteMsg::UpdateAcceptedBetDenoms {
            accepted_bet_denoms,
        } => execute_update_accepted_bet_denoms(deps, info, accepted_bet_denoms),
        ExecuteMsg::CreateRound { start_time, name } => {
            execute_create_round(deps, info, env, start_time, name)
        }
        ExecuteMsg::PlaceBet { side, round_name } => {
            execute_place_bet(deps, info, env, side, round_name)
        }
        ExecuteMsg::WithdrawBet { round_name } => execute_withdraw_bet(deps, info, env, round_name),
        ExecuteMsg::StartRound { name } => execute_start_round(deps, info, env, name),
        ExecuteMsg::StopRound { name } => execute_stop_round(deps, info, env, name),
        ExecuteMsg::ClaimWin { round_name } => execute_claim_win(deps, info, env, round_name),
        ExecuteMsg::WithdrawFromPool {
            to_address,
            denom,
            amount,
        } => execute_withdraw_from_treasury_pool(deps, info, env, denom, to_address, amount),
    }
}

// updates the list of admins who can call the contract e.g to start and stop a round
pub fn execute_update_admins(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    admins: Vec<String>,
) -> Result<Response, ContractError> {
    let is_admin = sender_is_admin(&deps, &info.sender.as_str())?;
    if !is_admin {
        return Err(ContractError::Unauthorized {});
    }
    let mut config = CONFIG.load(deps.storage)?;
    let admins = map_validate(deps.api, &admins)?;
    config.admins = admins;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "update admins"))
}

// updates the asset which users can bet on in the contract
pub fn execute_update_asset_denom(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    asset_denom: String,
) -> Result<Response, ContractError> {
    let is_admin = sender_is_admin(&deps, &info.sender.as_str())?;
    if !is_admin {
        return Err(ContractError::Unauthorized {});
    }
    let mut config = CONFIG.load(deps.storage)?;
    config.asset_denom = asset_denom;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "update asset denom"))
}

// updates the list of denoms accepted when betting
pub fn execute_update_accepted_bet_denoms(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    accepted_bet_denoms: Vec<String>,
) -> Result<Response, ContractError> {
    let is_admin = sender_is_admin(&deps, &info.sender.as_str())?;
    if !is_admin {
        return Err(ContractError::Unauthorized {});
    }
    let mut config = CONFIG.load(deps.storage)?;
    config.accepted_bet_denoms = accepted_bet_denoms;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "update accepted bet denoms"))
}

// creates a round that users can bet on, start_time is the time when the round should start and
// name is the name of the round, this can also be a unique id
pub fn execute_create_round(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    env: Env,
    start_time: u64,
    name: String,
) -> Result<Response, ContractError> {
    let current_time = env.block.time.seconds();
    let in_five_mins = current_time + 300;
    if start_time < in_five_mins {
        return Err(ContractError::InvalidStartTime {
            message: String::from(
                "start_time should be at least 5 mins away from round creation time",
            ),
        });
    }
    let stop_time = start_time + 300;
    let existing_round = ROUND.may_load(deps.storage, name.clone())?;
    match existing_round {
        Some(_round) => return Err(ContractError::RoundAlreadyExists {}),
        None => {
            let new_round = Round {
                created_at: current_time,
                creator: info.sender,
                start_time,
                stop_time,
                participants_count: 0,
                up_bets_count: 0,
                down_bets_count: 0,
                total_bet_amount: 0,
                total_up_bet_amount: 0,
                total_down_bet_amount: 0,
                is_started: false,
                started_at: None,
                is_stopped: false,
                stopped_at: None,
                start_price: None,
                stop_price: None,
                bet_denoms: Vec::new(),
            };
            ROUND.save(deps.storage, name, &new_round)?;
        }
    }
    Ok(Response::new().add_attribute("action", "Create round"))
}

// enables an admin to start a given round so that it can be initialised with a starting price
// name is the unique name of the round to be started
pub fn execute_start_round(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    env: Env,
    name: String,
) -> Result<Response, ContractError> {
    let is_admin = sender_is_admin(&deps, &info.sender.as_str())?;
    if !is_admin {
        return Err(ContractError::Unauthorized {});
    }
    let existing_round = ROUND.may_load(deps.storage, name.clone())?;
    match existing_round {
        Some(round) => {
            if round.is_started {
                return Err(ContractError::RoundAlreadyStarted {});
            }
            let current_time = env.block.time.seconds();
            if current_time > round.stop_time {
                return Err(ContractError::RoundStopTimePassed {});
            }
            let config = CONFIG.load(deps.storage)?;
            let q = KujiraQuerier::new(&deps.querier);
            let res = q.query_exchange_rate(config.asset_denom)?;
            let price = res.rate;
            let mut started_round = round;
            started_round.is_started = true;
            started_round.started_at = Some(current_time);
            started_round.start_price = Some(price);
            ROUND.save(deps.storage, name, &started_round)?;
        }
        None => return Err(ContractError::RoundDoesNotExist {}),
    }
    Ok(Response::new().add_attribute("action", "Start round"))
}

// enables a user to place a bet on a round
// side is the side enum variant representing the side that the user is betting on
pub fn execute_place_bet(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    env: Env,
    side: Side,
    round_name: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let funds_len = info.funds.len();

    if funds_len < 1 {
        return Err(ContractError::NoCoinsSent {});
    }
    if funds_len > 1 {
        return Err(ContractError::TooManyCoins {});
    }
    let coin = &info.funds[0];

    let denom_accepted = config.accepted_bet_denoms.contains(&coin.denom);

    if !denom_accepted {
        return Err(ContractError::DenomNotSupported {});
    }

    let existing_round = ROUND.may_load(deps.storage, round_name.clone())?;
    match existing_round {
        Some(round) => {
            let current_time = env.block.time.seconds();
            if round.start_time < current_time && round.is_started {
                return Err(ContractError::RoundAlreadyStarted {});
            }
            let coin = &info.funds[0];
            let sent_amount = coin.amount.u128();
            let existing_bet =
                BET.may_load(deps.storage, (round_name.clone(), info.sender.clone()))?;
            match existing_bet {
                Some(_bet) => return Err(ContractError::BetAlreadyPlaced {}),
                None => {
                    let new_bet = Bet {
                        side: side.clone(),
                        amount: sent_amount,
                        denom: coin.denom.clone(),
                        win_claimed: false,
                        placed_at: current_time,
                    };
                    BET.save(
                        deps.storage,
                        (round_name.clone(), info.sender.clone()),
                        &new_bet,
                    )?;
                    let bet_denom_in_previous_bets = round.bet_denoms.contains(&coin.denom);
                    let mut updated_round = round.clone();
                    let bet_denom = coin.denom.clone();
                    if !bet_denom_in_previous_bets {
                        let mut existing_bet_denoms = round.bet_denoms;
                        existing_bet_denoms.push(bet_denom);
                        updated_round.bet_denoms = existing_bet_denoms;
                    }
                    match side {
                        Side::Up => {
                            updated_round.up_bets_count += 1;
                            updated_round.total_up_bet_amount += sent_amount;
                        }
                        Side::Down => {
                            updated_round.down_bets_count += 1;
                            updated_round.total_down_bet_amount += sent_amount;
                        }
                    }
                    let existing_round_denom_bet = ROUNDDENOMBET
                        .may_load(deps.storage, (round_name.clone(), coin.denom.clone()))?;
                    match existing_round_denom_bet {
                        Some(round_denom_bet) => {
                            let mut updated_round_denom_bet = round_denom_bet;
                            updated_round_denom_bet.amount += sent_amount;
                            ROUNDDENOMBET.save(
                                deps.storage,
                                (round_name.clone(), coin.denom.clone()),
                                &updated_round_denom_bet,
                            )?;
                        }
                        None => {
                            let new_round_denom_bet = RoundDenomBet {
                                amount: sent_amount,
                            };
                            ROUNDDENOMBET.save(
                                deps.storage,
                                (round_name.clone(), coin.denom.clone()),
                                &new_round_denom_bet,
                            )?;
                        }
                    }

                    updated_round.total_bet_amount += sent_amount;
                    updated_round.participants_count += 1;
                    ROUND.save(deps.storage, round_name, &updated_round)?;
                }
            }
        }
        None => return Err(ContractError::RoundDoesNotExist {}),
    }
    Ok(Response::new().add_attribute("action", "place bet"))
}

pub fn execute_withdraw_bet(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    env: Env,
    round_name: String,
) -> Result<Response, ContractError> {
    let existing_round = ROUND.may_load(deps.storage, round_name.clone())?;
    let withdraw_message: CosmosMsg;
    match existing_round {
        Some(round) => {
            let current_time = env.block.time.seconds();
            if round.start_time < current_time || round.is_started {
                return Err(ContractError::RoundAlreadyStarted {});
            }
            let existing_bet =
                BET.may_load(deps.storage, (round_name.clone(), info.sender.clone()))?;

            match existing_bet {
                Some(bet) => {
                    let bet_coin = Coin {
                        denom: bet.denom.clone(),
                        amount: Uint128::from(bet.amount),
                    };

                    withdraw_message = CosmosMsg::Bank(BankMsg::Send {
                        to_address: info.sender.to_string(),
                        amount: vec![bet_coin],
                    });
                    let mut updated_round = round;
                    match bet.side {
                        Side::Up => {
                            updated_round.up_bets_count -= 1;
                            updated_round.total_up_bet_amount -= bet.amount;
                        }
                        Side::Down => {
                            updated_round.down_bets_count -= 1;
                            updated_round.total_down_bet_amount -= bet.amount;
                        }
                    }

                    updated_round.total_bet_amount -= bet.amount;
                    updated_round.participants_count -= 1;
                    ROUND.save(deps.storage, round_name.clone(), &updated_round)?;

                    let existing_round_denom_bet = ROUNDDENOMBET
                        .may_load(deps.storage, (round_name.clone(), bet.denom.clone()))?;
                    match existing_round_denom_bet {
                        Some(round_denom_bet) => {
                            let mut updated_round_denom_bet = round_denom_bet;
                            updated_round_denom_bet.amount -= bet.amount;
                            ROUNDDENOMBET.save(
                                deps.storage,
                                (round_name.clone(), bet.denom),
                                &updated_round_denom_bet,
                            )?;
                        }
                        None => {}
                    }
                    BET.remove(deps.storage, (round_name, info.sender))
                }
                None => return Err(ContractError::BetNotFound {}),
            }
        }
        None => return Err(ContractError::RoundDoesNotExist {}),
    }
    Ok(Response::new()
        .add_attribute("action", "withdraw bet")
        .add_message(withdraw_message))
}

// enables an admin to stop a round that is due based on the stop_time
// name here is the unique name of the round to be stopped
pub fn execute_stop_round(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    env: Env,
    name: String,
) -> Result<Response, ContractError> {
    let is_admin = sender_is_admin(&deps, &info.sender.as_str())?;
    if !is_admin {
        return Err(ContractError::Unauthorized {});
    }
    let existing_round = ROUND.may_load(deps.storage, name.clone())?;
    match existing_round {
        Some(round) => {
            if round.is_stopped {
                return Err(ContractError::RoundAlreadyEnded {});
            }
            let current_time = env.block.time.seconds();
            if current_time < round.stop_time {
                return Err(ContractError::RoundStillInProgress {});
            }
            let config = CONFIG.load(deps.storage)?;
            let q = KujiraQuerier::new(&deps.querier);
            let res = q.query_exchange_rate(config.asset_denom)?;
            let price = res.rate;
            let mut stopped_round = round.clone();
            stopped_round.is_stopped = true;
            stopped_round.stopped_at = Some(current_time);
            stopped_round.stop_price = Some(price);
            ROUND.save(deps.storage, name.clone(), &stopped_round)?;
            // if the price changed, take fees
            if round.start_price.unwrap() != price {
                // update the treasury pool amount for each denom used to bet in the round
                for denom in round.bet_denoms.clone() {
                    let existing_round_denom_bet =
                        ROUNDDENOMBET.may_load(deps.storage, (name.clone(), denom.clone()))?;
                    println!("denoms {:?}", denom.clone());
                    match existing_round_denom_bet {
                        Some(round_denom_bet) => {
                            let treasury_share = round_denom_bet.amount * 15 / 100;

                            println!("{}", treasury_share);
                            let existing_treasury_pool_denom =
                                TREASURYPOOLDENOM.may_load(deps.storage, denom.clone())?;
                            println!("existing denom{:?}", existing_treasury_pool_denom.clone());
                            match existing_treasury_pool_denom {
                                Some(treasury_pool_denom) => {
                                    println!("existing {:?}", treasury_pool_denom.clone());
                                    let mut updated_treasury_pool_denom = treasury_pool_denom;
                                    updated_treasury_pool_denom.amount += treasury_share;
                                    TREASURYPOOLDENOM.save(
                                        deps.storage,
                                        denom,
                                        &updated_treasury_pool_denom,
                                    )?;
                                }
                                None => {
                                    let new_treasury_pool_denom = TreasuryPoolDenom {
                                        amount: treasury_share,
                                        denom: denom.clone(),
                                    };

                                    println!("new {:?}", new_treasury_pool_denom);
                                    TREASURYPOOLDENOM.save(
                                        deps.storage,
                                        denom,
                                        &new_treasury_pool_denom,
                                    )?
                                }
                            }
                        }
                        None => continue,
                    }
                }
            }
        }
        None => return Err(ContractError::RoundDoesNotExist {}),
    }
    Ok(Response::new().add_attribute("action", "Stop round"))
}

// enables a user to claim their win from a given round
// this function also sends fees from the round to the treasury address if
// the fees have not been claimed already
pub fn execute_claim_win(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    _env: Env,
    round_name: String,
) -> Result<Response, ContractError> {
    let existing_round = ROUND.may_load(deps.storage, round_name.clone())?;
    let mut messages: Vec<CosmosMsg> = Vec::new();
    match existing_round {
        Some(round) => {
            if !round.is_stopped {
                return Err(ContractError::RoundStillInProgress {});
            }

            let existing_bet =
                BET.may_load(deps.storage, (round_name.clone(), info.sender.clone()))?;
            let start_price = round.start_price.unwrap();
            let stop_price = round.stop_price.unwrap();
            match existing_bet {
                Some(bet) => {
                    let mut is_winner = false;
                    match bet.side {
                        Side::Up => {
                            if stop_price > start_price {
                                is_winner = true
                            }
                        }
                        Side::Down => {
                            if stop_price < start_price {
                                is_winner = true
                            }
                        }
                    }
                    let mut sender_coins: Vec<Coin> = Vec::new();
                    if is_winner {
                        if bet.win_claimed {
                            return Err(ContractError::WinAlreadyClaimed {});
                        }
                        // sharable amount is 85% of the bets, 15% goes to fees wallet
                        let sharable_amount = 85 / 100 * round.total_bet_amount;
                        if round.participants_count == 1 {
                            // if the sender was the only participant he gets 20% of bet
                            // amount back if he wins
                            let win_amount = 20 / 100 * bet.amount;
                            let sender_coin = Coin {
                                denom: bet.denom.clone(),
                                amount: Uint128::from(win_amount),
                            };
                            sender_coins.push(sender_coin);
                        } else {
                            let senders_share = bet.amount / sharable_amount;
                            for denom in round.bet_denoms.clone() {
                                let existing_round_denom_bet = ROUNDDENOMBET
                                    .may_load(deps.storage, (round_name.clone(), denom.clone()))?;
                                match existing_round_denom_bet {
                                    Some(round_denom_bet) => {
                                        let denom_win_amount =
                                            senders_share * round_denom_bet.amount;
                                        let sender_coin = Coin {
                                            denom: denom.clone(),
                                            amount: Uint128::from(denom_win_amount),
                                        };
                                        sender_coins.push(sender_coin);
                                    }
                                    None => continue,
                                }
                            }
                        }

                        let sender_wins_msg = CosmosMsg::Bank(BankMsg::Send {
                            to_address: info.sender.to_string(),
                            amount: sender_coins,
                        });

                        messages.push(sender_wins_msg);
                        let mut updated_bet = bet;
                        updated_bet.win_claimed = true;
                        BET.save(
                            deps.storage,
                            (round_name.clone(), info.sender),
                            &updated_bet,
                        )?;
                    } else if start_price == stop_price {
                        let sender_coin = Coin {
                            denom: bet.denom.clone(),
                            amount: Uint128::from(bet.amount),
                        };
                        sender_coins.push(sender_coin);
                        let prices_equal_msg = CosmosMsg::Bank(BankMsg::Send {
                            to_address: info.sender.to_string(),
                            amount: sender_coins,
                        });
                        messages.push(prices_equal_msg)
                    } else {
                        return Err(ContractError::YouLost {});
                    }
                }
                None => return Err(ContractError::BetNotFound {}),
            }
        }
        None => return Err(ContractError::RoundDoesNotExist {}),
    }
    Ok(Response::new()
        .add_attribute("action", "claim win")
        .add_messages(messages))
}

// this enables an admin to withdraw available funds from the treasury pool
pub fn execute_withdraw_from_treasury_pool(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    _env: Env,
    denom: String,
    to_address: String,
    amount: u128,
) -> Result<Response, ContractError> {
    let is_admin = sender_is_admin(&deps, &info.sender.as_str())?;
    if !is_admin {
        return Err(ContractError::Unauthorized {});
    }
    let existing_treasury_pool_denom = TREASURYPOOLDENOM.may_load(deps.storage, denom.clone())?;
    let message: CosmosMsg;
    match existing_treasury_pool_denom {
        Some(treasury_pool_denom) => {
            if treasury_pool_denom.amount < amount {
                return Err(ContractError::InsufficientTreasuryDenomBalance {});
            }
            let coin = Coin {
                denom,
                amount: Uint128::from(amount),
            };
            message = CosmosMsg::Bank(BankMsg::Send {
                to_address,
                amount: vec![coin],
            });
        }
        None => return Err(ContractError::TreasuryDenomDoesNotExist {}),
    }
    Ok(Response::new()
        .add_attribute("action", "Withdraw from treasury pool")
        .add_message(message))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetRounds {} => query_all_rounds(deps, env),
        QueryMsg::GetRound { round_name } => query_round(deps, env, round_name),
        QueryMsg::GetTreasuryPoolDenom { denom } => query_treasury_pool_denom(deps, env, denom),
        QueryMsg::GetUserBet {
            round_name,
            user_addr,
        } => query_user_bet(deps, env, round_name, user_addr),
    }
}

// gets all rounds created in the smart contract
pub fn query_all_rounds(deps: Deps<KujiraQuery>, _env: Env) -> StdResult<Binary> {
    let rounds = ROUND
        .range(deps.storage, None, None, Order::Ascending)
        .map(|p| Ok(p?.1))
        .collect::<StdResult<Vec<_>>>()?;
    to_binary(&AllRoundsResponse { rounds })
}

// gets single treasury pool denom
pub fn query_treasury_pool_denom(
    deps: Deps<KujiraQuery>,
    _env: Env,
    denom: String,
) -> StdResult<Binary> {
    let treasury_pool_denom = TREASURYPOOLDENOM.may_load(deps.storage, denom)?;
    to_binary(&TreasuryPoolDenomResponse {
        treasury_pool_denom,
    })
}

// gets single round by name
pub fn query_round(deps: Deps<KujiraQuery>, _env: Env, round_name: String) -> StdResult<Binary> {
    let round = ROUND.may_load(deps.storage, round_name)?;
    to_binary(&RoundResponse { round })
}

// gets bets placed by a given user in a given round
pub fn query_user_bet(
    deps: Deps<KujiraQuery>,
    _env: Env,
    round_name: String,
    user_addr: String,
) -> StdResult<Binary> {
    let validated_user_addr = deps.api.addr_validate(&user_addr)?;
    let bet = BET.may_load(deps.storage, (round_name, validated_user_addr))?;
    to_binary(&UserBetResponse { bet })
}

#[cfg(test)]
mod tests {

    use crate::contract::{execute, instantiate, query};
    use crate::msg::{
        AllRoundsResponse, ExecuteMsg, InstantiateMsg, QueryMsg, RoundResponse,
        TreasuryPoolDenomResponse, UserBetResponse,
    };
    use crate::state::{Bet, Round, Side, TreasuryPoolDenom};
    use crate::ContractError;
    use core::cell::RefCell;
    use core::marker::PhantomData;
    use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockQuerier, MockStorage};
    use cosmwasm_std::{
        attr, from_binary, to_binary, Coin, ContractResult, Decimal, OwnedDeps, SystemResult,
        Timestamp, Uint128,
    };
    use kujira::query::{ExchangeRateResponse, KujiraQuery, OracleQuery};
    use std::collections::HashMap;

    use std::str::FromStr;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub const ADMIN1: &str = "addr1";
    pub const ADMIN2: &str = "addr2";
    pub const ANYONE: &str = "anyone";

    pub const USER1: &str = "user1";

    pub const TREASURY: &str = "treasury1";

    pub const ASSETDENOM: &str = "asset1";
    pub const ASSETDENOM2: &str = "asset2";

    pub const DENOM1: &str = "denom1";
    pub const DENOM2: &str = "denom2";
    pub const DENOM3: &str = "denom3";

    thread_local! {
        static PRICES: RefCell<HashMap<String, Decimal>> = RefCell::new(HashMap::new());
    }

    type OwnedDepsType = OwnedDeps<MockStorage, MockApi, MockQuerier<KujiraQuery>, KujiraQuery>;

    pub fn mock_dependencies_kujira() -> OwnedDepsType {
        let querier = MockQuerier::new(&[]).with_custom_handler(|query| match query {
            // KujiraQuery::Oracle(OracleQuery::ExchangeRate { denom }) => {
            //     let price = PRICES.with(|p| *p.borrow().get(denom.as_str()).unwrap());
            //     SystemResult::Ok(ContractResult::Ok(
            //         to_binary(&ExchangeRateResponse { rate: price }).unwrap(),
            //     ))
            // }
            KujiraQuery::Oracle(OracleQuery::ExchangeRate { denom: _ }) => {
                let exchange_rate_response = ExchangeRateResponse {
                    rate: Decimal::from_str("1.23").unwrap(),
                };
                SystemResult::Ok(ContractResult::Ok(
                    to_binary(&exchange_rate_response).unwrap(),
                ))
            }
            _ => panic!("Unexpected query: {query:?}"),
        });

        OwnedDeps {
            storage: MockStorage::default(),
            api: MockApi::default(),
            querier,
            custom_query_type: PhantomData,
        }
    }

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.attributes, vec![attr("action", "instantiate")])
    }

    #[test]
    fn test_execute_update_admins() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        //
        let msg = ExecuteMsg::UpdateAdmins {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string(), ANYONE.to_string()],
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.attributes, vec![attr("action", "update admins")])
    }

    #[test]
    fn test_execute_update_asset_denom() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        let msg = ExecuteMsg::UpdateAssetDenom {
            asset_denom: ASSETDENOM2.to_string(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.attributes, vec![attr("action", "update asset denom")])
    }

    #[test]
    fn test_execute_update_accepted_bet_denom() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        let msg = ExecuteMsg::UpdateAcceptedBetDenoms {
            accepted_bet_denoms: vec![
                String::from(DENOM1),
                String::from(DENOM2),
                String::from(DENOM3),
            ],
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(
            res.attributes,
            vec![attr("action", "update accepted bet denoms")]
        )
    }

    #[test]
    fn test_execute_create_round() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.attributes, vec![attr("action", "Create round")])
    }

    #[test]
    fn test_execute_place_bet_with_accepted_denom() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.attributes, vec![attr("action", "place bet")])
    }

    #[test]
    fn test_execute_place_bet_with_unaccepted_denom() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: "RANDOMDENOM".to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();

        assert!(matches!(err, ContractError::DenomNotSupported {}))
    }

    #[test]
    fn test_execute_withdraw_bet() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::WithdrawBet {
            round_name: "Round1".to_string(),
        };

        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.attributes, vec![attr("action", "withdraw bet")])
    }

    #[test]
    fn test_execute_start_round_as_admin() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let info = mock_info(ADMIN1, &vec![]);

        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert_eq!(res.attributes, vec![attr("action", "Start round")])
    }

    #[test]
    fn test_execute_start_round_not_admin() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env, info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();

        assert!(matches!(err, ContractError::Unauthorized {}))
    }

    #[test]
    fn test_execute_stop_round_while_in_progress() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let info = mock_info(ADMIN1, &vec![]);

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StopRound {
            name: "Round1".to_string(),
        };

        let err = execute(deps.as_mut(), env, info.clone(), msg).unwrap_err();

        assert!(matches!(err, ContractError::RoundStillInProgress {}))
    }

    #[test]
    fn test_execute_stop_round() {
        let mut deps = mock_dependencies_kujira();
        let mut env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let info = mock_info(ADMIN1, &vec![]);

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StopRound {
            name: "Round1".to_string(),
        };

        let twelve_minutes = Duration::from_secs(12 * 60); // 6 minutes in seconds
        let stop_timestamp = unix_timestamp + twelve_minutes.as_secs();
        env.block.time = Timestamp::from_seconds(stop_timestamp);

        let res = execute(deps.as_mut(), env, info.clone(), msg).unwrap();

        assert_eq!(res.attributes, vec![attr("action", "Stop round")])
    }

    #[test]
    fn test_execute_claim_win_of_existing_bet() {
        let mut deps = mock_dependencies_kujira();
        let mut env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let info = mock_info(ADMIN1, &vec![]);

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StopRound {
            name: "Round1".to_string(),
        };

        let twelve_minutes = Duration::from_secs(12 * 60); // 6 minutes in seconds
        let stop_timestamp = unix_timestamp + twelve_minutes.as_secs();
        env.block.time = Timestamp::from_seconds(stop_timestamp);

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::ClaimWin {
            round_name: "Round1".to_string(),
        };

        let info = mock_info(USER1, &vec![]);
        let res = execute(deps.as_mut(), env, info.clone(), msg).unwrap();

        assert_eq!(res.attributes, vec![attr("action", "claim win")])
    }

    #[test]
    fn test_execute_claim_win_of_nonexisting_bet() {
        let mut deps = mock_dependencies_kujira();
        let mut env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let info = mock_info(ADMIN1, &vec![]);

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StopRound {
            name: "Round1".to_string(),
        };

        let twelve_minutes = Duration::from_secs(12 * 60); // 6 minutes in seconds
        let stop_timestamp = unix_timestamp + twelve_minutes.as_secs();
        env.block.time = Timestamp::from_seconds(stop_timestamp);

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::ClaimWin {
            round_name: "Round1".to_string(),
        };

        let err = execute(deps.as_mut(), env, info.clone(), msg).unwrap_err();

        assert!(matches!(err, ContractError::BetNotFound {}))
    }

    #[test]
    fn test_execute_withdraw_from_treasury_pool_when_there_are_no_fees() {
        let mut deps = mock_dependencies_kujira();
        let mut env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let info = mock_info(ADMIN1, &vec![]);

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StopRound {
            name: "Round1".to_string(),
        };

        let twelve_minutes = Duration::from_secs(12 * 60); // 6 minutes in seconds
        let stop_timestamp = unix_timestamp + twelve_minutes.as_secs();
        env.block.time = Timestamp::from_seconds(stop_timestamp);

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::WithdrawFromPool {
            to_address: TREASURY.to_string(),
            denom: DENOM1.to_string(),
            amount: 1,
        };

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();

        assert!(matches!(err, ContractError::TreasuryDenomDoesNotExist {}))
    }

    #[test]
    fn test_execute_withdraw_from_treasury_pool_when_fees_exist() {
        let mut deps = mock_dependencies_kujira();
        let mut env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let info = mock_info(ADMIN1, &vec![]);

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StopRound {
            name: "Round1".to_string(),
        };

        let mut querier: MockQuerier<KujiraQuery> = MockQuerier::new(&[]);
        // update querier to have price change
        querier = querier.with_custom_handler(|query: &KujiraQuery| match query {
            KujiraQuery::Oracle(OracleQuery::ExchangeRate { denom: _ }) => {
                let exchange_rate_response = ExchangeRateResponse {
                    rate: Decimal::from_str("1.10").unwrap(),
                };
                SystemResult::Ok(ContractResult::Ok(
                    to_binary(&exchange_rate_response).unwrap(),
                ))
            }
            _ => unimplemented!(),
        });
        deps.querier = querier;

        let twelve_minutes = Duration::from_secs(12 * 60); // 6 minutes in seconds
        let stop_timestamp = unix_timestamp + twelve_minutes.as_secs();
        env.block.time = Timestamp::from_seconds(stop_timestamp);

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::WithdrawFromPool {
            to_address: TREASURY.to_string(),
            denom: DENOM1.to_string(),
            amount: 1,
        };

        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        assert_eq!(
            res.attributes,
            vec![attr("action", "Withdraw from treasury pool")]
        )
    }

    #[test]
    fn test_query_get_rounds() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = QueryMsg::GetRounds {};

        let bin = query(deps.as_ref(), env.clone(), msg).unwrap();

        let res: AllRoundsResponse = from_binary(&bin).unwrap();

        let current_time = env.block.time.seconds();

        let stop_time = new_timestamp + 300;
        let round = Round {
            created_at: current_time,
            creator: info.sender,
            start_time: new_timestamp,
            stop_time,
            participants_count: 0,
            up_bets_count: 0,
            down_bets_count: 0,
            total_bet_amount: 0,
            total_up_bet_amount: 0,
            total_down_bet_amount: 0,
            is_started: false,
            started_at: None,
            is_stopped: false,
            stopped_at: None,
            start_price: None,
            stop_price: None,
            bet_denoms: Vec::new(),
        };

        assert_eq!(res.rounds, vec![round]);
    }

    #[test]
    fn test_query_get_round() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = QueryMsg::GetRound {
            round_name: "Round1".to_string(),
        };

        let bin = query(deps.as_ref(), env.clone(), msg).unwrap();

        let res: RoundResponse = from_binary(&bin).unwrap();

        let current_time = env.block.time.seconds();

        let stop_time = new_timestamp + 300;
        let round = Round {
            created_at: current_time,
            creator: info.sender,
            start_time: new_timestamp,
            stop_time,
            participants_count: 0,
            up_bets_count: 0,
            down_bets_count: 0,
            total_bet_amount: 0,
            total_up_bet_amount: 0,
            total_down_bet_amount: 0,
            is_started: false,
            started_at: None,
            is_stopped: false,
            stopped_at: None,
            start_price: None,
            stop_price: None,
            bet_denoms: Vec::new(),
        };

        assert_eq!(res.round, Some(round));
    }

    #[test]
    fn test_query_user_bet() {
        let mut deps = mock_dependencies_kujira();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(1000u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = QueryMsg::GetUserBet {
            round_name: "Round1".to_string(),
            user_addr: USER1.to_string(),
        };

        let bin = query(deps.as_ref(), env.clone(), msg).unwrap();

        let res: UserBetResponse = from_binary(&bin).unwrap();
        let current_time = env.block.time.seconds();
        let new_bet = Bet {
            side: Side::Up,
            amount: 1000u128,
            denom: DENOM1.to_string(),
            win_claimed: false,
            placed_at: current_time,
        };

        assert_eq!(res.bet, Some(new_bet));
    }

    #[test]
    fn test_query_treasury_pool_denom() {
        let mut deps = mock_dependencies_kujira();
        let mut env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
        };

        let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let current_time = SystemTime::now();
        let unix_timestamp = current_time
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get UNIX timestamp")
            .as_secs();

        let six_minutes = Duration::from_secs(6 * 60); // 6 minutes in seconds
        let new_timestamp = unix_timestamp + six_minutes.as_secs();

        let msg = ExecuteMsg::CreateRound {
            start_time: new_timestamp,
            name: "Round1".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::PlaceBet {
            side: Side::Up,
            round_name: "Round1".to_string(),
        };

        let info = mock_info(
            USER1,
            &vec![Coin {
                denom: DENOM1.to_string(),
                amount: Uint128::from(100u128),
            }],
        );

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StartRound {
            name: "Round1".to_string(),
        };

        let info = mock_info(ADMIN1, &vec![]);

        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::StopRound {
            name: "Round1".to_string(),
        };

        let mut querier: MockQuerier<KujiraQuery> = MockQuerier::new(&[]);
        // update querier to have price change
        querier = querier.with_custom_handler(|query: &KujiraQuery| match query {
            KujiraQuery::Oracle(OracleQuery::ExchangeRate { denom: _ }) => {
                let exchange_rate_response = ExchangeRateResponse {
                    rate: Decimal::from_str("1.10").unwrap(),
                };
                SystemResult::Ok(ContractResult::Ok(
                    to_binary(&exchange_rate_response).unwrap(),
                ))
            }
            _ => unimplemented!(),
        });
        deps.querier = querier;

        let twelve_minutes = Duration::from_secs(12 * 60); // 6 minutes in seconds
        let stop_timestamp = unix_timestamp + twelve_minutes.as_secs();
        env.block.time = Timestamp::from_seconds(stop_timestamp);

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let msg = QueryMsg::GetTreasuryPoolDenom {
            denom: DENOM1.to_string(),
        };

        let bin = query(deps.as_ref(), env.clone(), msg).unwrap();

        let res: TreasuryPoolDenomResponse = from_binary(&bin).unwrap();

        let new_treasury_pool_denom = TreasuryPoolDenom {
            amount: 15u128,
            denom: DENOM1.to_string(),
        };

        assert_eq!(res.treasury_pool_denom, Some(new_treasury_pool_denom));
    }
}
