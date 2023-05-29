use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::{Bet, Round, Side, TreasuryPoolDenom};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub admins: Vec<String>,
    pub asset_denom: String,
    pub accepted_bet_denoms: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    UpdateAdmins {
        admins: Vec<String>,
    },
    CreateRound {
        start_time: u64,
        name: String,
    },
    PlaceBet {
        side: Side,
        round_name: String,
    },
    WithdrawBet {
        round_name: String,
    },
    StartRound {
        name: String,
    },
    StopRound {
        name: String,
    },
    ClaimWin {
        round_name: String,
    },
    WithdrawFromPool {
        to_address: String,
        denom: String,
        amount: u128,
    },
    UpdateAcceptedBetDenoms {
        accepted_bet_denoms: Vec<String>,
    },
    UpdateAssetDenom {
        asset_denom: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetRounds {},
    GetTreasuryPoolDenom {
        denom: String,
    },
    GetRound {
        round_name: String,
    },
    GetUserBet {
        round_name: String,
        user_addr: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllRoundsResponse {
    pub rounds: Vec<Round>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TreasuryPoolDenomResponse {
    pub treasury_pool_denom: Option<TreasuryPoolDenom>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct RoundResponse {
    pub round: Option<Round>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct UserBetResponse {
    pub bet: Option<Bet>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrateMsg {}
