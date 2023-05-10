#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Api, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{AllRoundsResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Bet, Config, Round, RoundDenomBet, Side, BET, CONFIG, ROUND, ROUNDDENOMBET};
use kujira::querier::KujiraQuerier;
use kujira::query::KujiraQuery;

const CONTRACT_NAME: &str = "crates.io:prediction-game";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let validated_treasury_addr = deps.api.addr_validate(&msg.treasury_addr)?;
    let config = Config {
        admins: map_validate(deps.api, &msg.admins)?,
        asset_denom: msg.asset_denom,
        treasury_addr: validated_treasury_addr,
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
) -> Result<Response<BankMsg>, ContractError> {
    match msg {
        ExecuteMsg::UpdateAdmins { admins } => execute_update_admins(deps, info, admins),
        ExecuteMsg::CreateRound { start_time, name } => {
            execute_create_round(deps, info, env, start_time, name)
        }
        ExecuteMsg::PlaceBet { side, round_name } => {
            execute_place_bet(deps, info, env, side, round_name)
        }
        ExecuteMsg::StartRound { name } => execute_start_round(deps, info, env, name),
        ExecuteMsg::StopRound { name } => execute_stop_round(deps, info, env, name),
        ExecuteMsg::ClaimWin { round_name } => execute_claim_win(deps, info, env, round_name),
        ExecuteMsg::ClaimRoundFees { round_name } => {
            execute_claim_round_fees(deps, info, env, round_name)
        }
        ExecuteMsg::UpdateTreasuryAddr { new_address } => {
            execute_update_treasury_address(deps, info, new_address)
        }
    }
}

// updates the list of admins who can call the contract e.g to start and stop a round
pub fn execute_update_admins(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    admins: Vec<String>,
) -> Result<Response<BankMsg>, ContractError> {
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

// updates the treasury address which recieves fees from a round
pub fn execute_update_treasury_address(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    new_address: String,
) -> Result<Response<BankMsg>, ContractError> {
    let is_admin = sender_is_admin(&deps, &info.sender.as_str())?;
    if !is_admin {
        return Err(ContractError::Unauthorized {});
    }
    let mut config = CONFIG.load(deps.storage)?;
    let new_addr = deps.api.addr_validate(&new_address)?;
    config.treasury_addr = new_addr;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "update treasury address"))
}

// creates a round that users can bet on, start_time is the time when the round should start and
// name is the name of the round, this can also be a unique id
pub fn execute_create_round(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    env: Env,
    start_time: u64,
    name: String,
) -> Result<Response<BankMsg>, ContractError> {
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
                total_withdrawn_amount: 0,
                is_started: false,
                started_at: None,
                is_stopped: false,
                stopped_at: None,
                start_price: None,
                stop_price: None,
                bet_denoms: Vec::new(),
                fees_claimed: false,
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
) -> Result<Response<BankMsg>, ContractError> {
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
) -> Result<Response<BankMsg>, ContractError> {
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

// enables an admin to stop a round that is due based on the stop_time
// name here is the unique name of the round to be stopped
pub fn execute_stop_round(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    env: Env,
    name: String,
) -> Result<Response<BankMsg>, ContractError> {
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
            let mut stopped_round = round;
            stopped_round.is_stopped = true;
            stopped_round.stopped_at = Some(current_time);
            stopped_round.stop_price = Some(price);
            ROUND.save(deps.storage, name, &stopped_round)?;
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
    env: Env,
    round_name: String,
) -> Result<Response<BankMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let existing_round = ROUND.may_load(deps.storage, round_name.clone())?;
    let mut messages: Vec<CosmosMsg<BankMsg>> = Vec::new();
    match existing_round {
        Some(round) => {
            let current_time = env.block.time.seconds();
            if current_time > round.stop_time || !round.is_stopped {
                return Err(ContractError::RoundStillInProgress {});
            }

            let existing_bet =
                BET.may_load(deps.storage, (round_name.clone(), info.sender.clone()))?;
            match existing_bet {
                Some(bet) => {
                    let mut is_winner = false;
                    match bet.side {
                        Side::Up => {
                            if round.stop_price > round.start_price {
                                is_winner = true
                            }
                        }
                        Side::Down => {
                            if round.stop_price < round.start_price {
                                is_winner = true
                            }
                        }
                    }
                    let mut sender_coins: Vec<Coin> = Vec::new();
                    let mut treasury_coins: Vec<Coin> = Vec::new();
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
                            let fee_amount = 15 / 100 * bet.amount;
                            let sender_coin = Coin {
                                denom: bet.denom.clone(),
                                amount: Uint128::from(win_amount),
                            };
                            sender_coins.push(sender_coin);

                            if !round.fees_claimed {
                                let fee_coin = Coin {
                                    denom: bet.denom.clone(),
                                    amount: Uint128::from(fee_amount),
                                };
                                treasury_coins.push(fee_coin)
                            }
                        } else {
                            let senders_share = bet.amount / sharable_amount;
                            for denom in round.bet_denoms.clone() {
                                let existing_round_denom_bet = ROUNDDENOMBET
                                    .may_load(deps.storage, (round_name.clone(), denom.clone()))?;
                                match existing_round_denom_bet {
                                    Some(round_denom_bet) => {
                                        let denom_win_amount =
                                            senders_share * round_denom_bet.amount;
                                        let fee_amount = 15 / 100 * round_denom_bet.amount;
                                        let sender_coin = Coin {
                                            denom: denom.clone(),
                                            amount: Uint128::from(denom_win_amount),
                                        };
                                        sender_coins.push(sender_coin);
                                        if !round.fees_claimed {
                                            let fee_coin = Coin {
                                                denom,
                                                amount: Uint128::from(fee_amount),
                                            };
                                            treasury_coins.push(fee_coin)
                                        }
                                    }
                                    None => continue,
                                }
                            }
                        }

                        let sender_wins_msg: CosmosMsg<BankMsg> = CosmosMsg::Bank(BankMsg::Send {
                            to_address: info.sender.to_string(),
                            amount: sender_coins,
                        });

                        let treasury_fees_msg: CosmosMsg<BankMsg> =
                            CosmosMsg::Bank(BankMsg::Send {
                                to_address: config.treasury_addr.to_string(),
                                amount: treasury_coins,
                            });
                        messages.push(sender_wins_msg);
                        messages.push(treasury_fees_msg);
                        let mut updated_bet = bet;
                        updated_bet.win_claimed = true;
                        BET.save(
                            deps.storage,
                            (round_name.clone(), info.sender),
                            &updated_bet,
                        )?;
                        let mut updated_round = round;
                        updated_round.fees_claimed = true;
                        ROUND.save(deps.storage, round_name, &updated_round)?;
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

// this enables an admin to claim fees from a given round
pub fn execute_claim_round_fees(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    _env: Env,
    round_name: String,
) -> Result<Response<BankMsg>, ContractError> {
    let is_admin = sender_is_admin(&deps, &info.sender.as_str())?;
    if !is_admin {
        return Err(ContractError::Unauthorized {});
    }
    let config = CONFIG.load(deps.storage)?;
    let existing_round = ROUND.may_load(deps.storage, round_name.clone())?;
    let message: CosmosMsg<BankMsg>;
    match existing_round {
        Some(round) => {
            if round.fees_claimed {
                return Err(ContractError::FeesAlreadyClaimed {});
            }
            let mut treasury_coins: Vec<Coin> = Vec::new();
            for denom in round.bet_denoms.clone() {
                let existing_round_denom_bet =
                    ROUNDDENOMBET.may_load(deps.storage, (round_name.clone(), denom.clone()))?;
                match existing_round_denom_bet {
                    Some(round_denom_bet) => {
                        let fee_amount = 15 / 100 * round_denom_bet.amount;
                        let fee_coin = Coin {
                            denom,
                            amount: Uint128::from(fee_amount),
                        };
                        treasury_coins.push(fee_coin)
                    }
                    None => continue,
                }
            }
            message = CosmosMsg::Bank(BankMsg::Send {
                to_address: config.treasury_addr.to_string(),
                amount: treasury_coins,
            });
            let mut updated_round = round;
            updated_round.fees_claimed = true;
            ROUND.save(deps.storage, round_name, &updated_round)?
        }
        None => return Err(ContractError::RoundDoesNotExist {}),
    }
    Ok(Response::new()
        .add_attribute("action", "claim fees")
        .add_message(message))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetRounds {} => query_all_rounds(deps, env),
    }
}

pub fn query_all_rounds(deps: Deps, _env: Env) -> StdResult<Binary> {
    let rounds = ROUND
        .range(deps.storage, None, None, Order::Ascending)
        .map(|p| Ok(p?.1))
        .collect::<StdResult<Vec<_>>>()?;
    to_binary(&AllRoundsResponse { rounds })
}

#[cfg(test)]
mod tests {

    use crate::contract::instantiate;
    use crate::msg::InstantiateMsg;
    use cosmwasm_std::attr;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};

    pub const ADMIN1: &str = "addr1";
    pub const ADMIN2: &str = "addr2";

    pub const TREASURY1: &str = "treasury1";

    pub const ASSETDENOM: &str = "asset1";

    pub const DENOM1: &str = "denom1";
    pub const DENOM2: &str = "denom2";

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info(ADMIN1, &vec![]);

        let msg = InstantiateMsg {
            admins: vec![ADMIN1.to_string(), ADMIN2.to_string()],
            asset_denom: ASSETDENOM.to_string(),
            accepted_bet_denoms: vec![String::from(DENOM1), String::from(DENOM2)],
            treasury_addr: TREASURY1.to_string(),
        };

        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.attributes, vec![attr("action", "instantiate")])
    }
}
