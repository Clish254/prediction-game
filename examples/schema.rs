use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use prediction_game::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use prediction_game::state::{Bet, Config, Round, RoundDenomBet, RoundParticipant};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(Config), &out_dir);
    export_schema(&schema_for!(Round), &out_dir);
    export_schema(&schema_for!(RoundParticipant), &out_dir);
    export_schema(&schema_for!(RoundDenomBet), &out_dir);
    export_schema(&schema_for!(Bet), &out_dir);
}
