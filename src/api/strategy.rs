use actix_web::web::Json;
use actix_web::{
    get, post,
    web::{self, scope},
    HttpResponse, Responder, Scope,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::bot::AppState;
use crate::strategy::strategy::{StrategyId, StrategySettings};
use crate::utils::time::string_to_timestamp;

#[derive(Debug, Deserialize)]
pub struct NewStrategyParams {
    symbol: String,
    strategy_name: String,
    algorithm_params: Value,
    interval: String,
    margin: Option<f64>,
    leverage: Option<u32>,
}
#[post("/new-strategy")]
async fn new_strategy(
    app_data: web::Data<AppState>,
    body: web::Json<NewStrategyParams>,
) -> impl Responder {
    let bot = app_data.bot.clone();

    let settings = StrategySettings {
        max_open_orders: 2,
        margin_usd: body.margin.unwrap_or_else(|| 1000.0),
        leverage: body.leverage.unwrap_or_else(|| 10),
    };

    let strategy_id = bot
        .lock()
        .await
        .add_strategy(
            &body.strategy_name,
            &body.symbol,
            &body.interval,
            settings,
            body.algorithm_params.clone(),
        )
        .await;

    match strategy_id {
        Ok(strategy_id) => {
            let json_data = json!({ "success": "Strategy started","strategy_id":strategy_id });

            HttpResponse::Ok().json(json_data)
        }
        Err(_e) => {
            let json_data = json!({ "error": "Unable to find strategy_name"});
            HttpResponse::ExpectationFailed().json(json_data)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StopStrategyParams {
    strategy_id: u32,
}
#[post("/stop-strategy")]
async fn stop_strategy(
    app_data: web::Data<AppState>,
    body: web::Json<StopStrategyParams>,
) -> impl Responder {
    let bot = app_data.bot.clone();

    let strategy_id = bot.lock().await.stop_strategy(body.strategy_id).await;

    let json_data = json!({ "success": "Strategy stopped","strategy_id":strategy_id });

    HttpResponse::Ok().json(json_data)
}

#[get("/active-strategies")]
async fn get_strategy_ids(app_data: web::Data<AppState>) -> impl Responder {
    let bot = app_data.bot.clone();

    let strategies = bot.lock().await.get_strategy_ids();

    let json_data = json!({ "strategies": strategies });

    HttpResponse::Ok().json(json_data)
}

#[get("/stop-all-strategies")]
async fn stop_all_strategies(app_data: web::Data<AppState>) -> impl Responder {
    let bot = app_data.bot.clone();

    let strategies = bot.lock().await.get_strategy_ids();

    for id in &strategies {
        bot.lock().await.stop_strategy(*id).await;
    }

    let json_data = json!({ "strategies_stopped": strategies });

    HttpResponse::Ok().json(json_data)
}

#[derive(Debug, Deserialize)]
pub struct SetStrategyParams {
    strategy_id: StrategyId,
    params: Value,
}
#[post("/set-strategy-params")]
async fn set_strategy_params(
    app_data: web::Data<AppState>,
    body: Json<SetStrategyParams>,
) -> impl Responder {
    if let Some(strategy) = app_data.bot.lock().await.get_strategy(body.strategy_id) {
        if let Err(err) = strategy.set_algorithm_params(body.params.clone()).await {
            let json_data = json!({ "error": err.to_string() });
            HttpResponse::Ok().json(json_data)
        } else {
            let updated_params = strategy.get_algorithm_params().await;
            let json_data = json!({ "success": { "updated_params": updated_params } });
            HttpResponse::Ok().json(json_data)
        }
    } else {
        let json_data = json!({ "error": "Unable to find strategy" });
        HttpResponse::Ok().json(json_data)
    }
}

#[derive(Debug, Deserialize)]
pub struct RunBackTestParams {
    symbol: String,
    strategy_name: String,
    algorithm_params: Value,
    interval: String,
    margin: Option<f64>,
    leverage: Option<u32>,
    from_ts: String,
    to_ts: String,
}
#[post("/run-back-test")]
async fn run_back_test(
    app_data: web::Data<AppState>,
    body: Json<RunBackTestParams>,
) -> impl Responder {
    let bot = app_data.bot.clone();
    let settings = StrategySettings {
        max_open_orders: 2,
        margin_usd: body.margin.unwrap_or_else(|| 1000.0),
        leverage: body.leverage.unwrap_or_else(|| 10),
    };

    let from_ts = string_to_timestamp(&body.from_ts);
    let to_ts = string_to_timestamp(&body.to_ts);
    if from_ts.is_err() || to_ts.is_err() {
        let json_data = json!({ "error": "Unable to parse dates".to_string()});
        return HttpResponse::ExpectationFailed().json(json_data);
    }

    // SAFETY: Error check above
    let from_ts = from_ts.unwrap();
    let to_ts = to_ts.unwrap();

    let result = bot
        .lock()
        .await
        .run_back_test(
            &body.strategy_name,
            &body.symbol,
            &body.interval,
            from_ts,
            to_ts,
            settings,
            body.algorithm_params.clone(),
        )
        .await;

    match result {
        Ok(result) => {
            let json_data = json!({ "result": result });

            HttpResponse::Ok().json(json_data)
        }
        Err(e) => {
            let json_data = json!({ "error": e.to_string()});
            HttpResponse::ExpectationFailed().json(json_data)
        }
    }
}

pub fn register_strategy_service() -> Scope {
    scope("/strategy")
        .service(new_strategy)
        .service(stop_strategy)
        .service(get_strategy_ids)
        .service(stop_all_strategies)
        .service(set_strategy_params)
        .service(run_back_test)
}
