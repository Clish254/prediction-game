use cw_utils::NativeBalance;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    // contract admins allowed to call private functions
    pub admins: Vec<Addr>,
    // denom of the asset users will be betting on
    pub asset_denom: String,
    // denoms that users are allowed to bet with
    pub accepted_bet_denoms: Vec<String>,
}

impl Config {
    /// returns true if the address is a registered admin
    pub fn is_admin(&self, addr: impl AsRef<str>) -> bool {
        let addr = addr.as_ref();
        self.admins.iter().any(|a| a.as_ref() == addr)
    }
}

pub const CONFIG: Item<Config> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Side {
    Up,
    Down,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Round {
    pub created_at: u64,
    pub creator: Addr,
    pub is_started: bool,
    pub started_at: Option<u64>,
    pub is_stopped: bool,
    pub stopped_at: Option<u64>,
    pub start_time: u64,
    pub stop_time: u64,
    pub participants_count: u128,
    pub up_bets_count: u128,
    pub down_bets_count: u128,
    pub total_bet_amount: NativeBalance,
    pub total_up_bet_amount: NativeBalance,
    pub total_down_bet_amount: NativeBalance,
    pub start_price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
}

// string here is the name of the round
pub const ROUND: Map<String, Round> = Map::new("round");

// this stores a user's bet amount and side in a given round
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Bet {
    pub side: Side,
    pub amount: u128,
    pub denom: String,
    pub win_claimed: bool,
    pub placed_at: u64,
}

// string here is the name of the round the user is betting on
// Addr is the address of the user who is betting
pub const BET: Map<(String, Addr), Bet> = Map::new("bet");

// this is stores the total amount that has been collected in fees
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TreasuryBalance {
    pub balance: NativeBalance,
}

pub const TREASURYBALANCE: Item<TreasuryBalance> = Item::new("treasurybalance");
