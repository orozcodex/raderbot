use dotenv_codegen::dotenv;
use log::info;
use serde_json::Value;

use std::{collections::HashMap, sync::Arc};

use crate::{
    account::{
        account::Account,
        trade::{Position, TradeTx},
    },
    exchange::{api::ExchangeApi, bingx::BingXApi, mock::MockExchangeApi},
    market::{
        market::Market,
        messages::MarketMessage,
        types::{ArcMutex, ArcReceiver, ArcSender},
    },
    storage::{fs::FsStorageManager, manager::StorageManager},
    strategy::{
        backer::BackTest,
        signal::SignalManager,
        strategy::{Strategy, StrategyId, StrategyInfo, StrategySettings, StrategySummary},
        types::{AlgorithmError, SignalMessage},
    },
    utils::channel::build_arc_channel,
};

use tokio::task::JoinHandle;

pub struct RaderBot {
    pub market: ArcMutex<Market>,
    pub account: ArcMutex<Account>,
    pub exchange_api: Arc<Box<dyn ExchangeApi>>,
    signal_manager: ArcMutex<SignalManager>,
    strategy_handles: HashMap<StrategyId, JoinHandle<()>>,
    strategies: HashMap<StrategyId, Strategy>,
    strategy_rx: ArcReceiver<SignalMessage>,
    // TODO: Add to Arc<Box<dyn StorageManager>>
    // to have access to storage across bot, ie.
    // may need data base connection
    storage_manager: Arc<Box<dyn StorageManager>>,
    strategy_tx: ArcSender<SignalMessage>,
}

impl RaderBot {
    pub async fn new() -> Self {
        // create new Arc of exchange API
        let api_key = dotenv!("BINGX_API_KEY");
        let secret_key = dotenv!("BINGX_SECRET_KEY");
        let dry_run = dotenv!("DRY_RUN");

        // create new channel for stream handler and market to communicate
        let (market_tx, market_rx) = build_arc_channel::<MarketMessage>();

        let exchange_api: Arc<Box<dyn ExchangeApi>> = Arc::new(Box::new(BingXApi::new(
            api_key,
            secret_key,
            market_tx.clone(),
        )));

        // create new storage manager
        let storage_manager: Arc<Box<dyn StorageManager>> =
            Arc::new(Box::new(FsStorageManager::default()));

        // create new market to hold market data
        let market = Market::new(
            market_rx.clone(),
            exchange_api.clone(),
            storage_manager.clone(),
            true,
        )
        .await;

        let market = ArcMutex::new(market);

        let (account_exchange_api, dry_run) = if dry_run == "True" {
            let api: Arc<Box<dyn ExchangeApi>> = Arc::new(Box::new(MockExchangeApi {}));
            (api, true)
        } else {
            (exchange_api.clone(), false)
        };

        let account = Account::new(account_exchange_api, true, dry_run).await;

        let account = ArcMutex::new(account);

        let (strategy_tx, strategy_rx) = build_arc_channel::<SignalMessage>();

        let signal_manager = ArcMutex::new(SignalManager::new(account.clone(), market.clone()));

        let mut _self = Self {
            market,
            signal_manager,
            account,
            exchange_api: exchange_api.clone(),
            strategy_handles: HashMap::new(),
            strategies: HashMap::new(),
            strategy_rx,
            strategy_tx,
            storage_manager,
        };

        _self.init().await;

        _self
    }

    pub async fn start_strategy(
        &mut self,
        strategy_name: &str,
        symbol: &str,
        interval: &str,
        settings: StrategySettings,
        algorithm_params: Value,
    ) -> Result<StrategyInfo, AlgorithmError> {
        let market = self.market.clone();
        let strategy_tx = self.strategy_tx.clone();

        let mut strategy = Strategy::new(
            strategy_name,
            symbol,
            interval,
            strategy_tx,
            market,
            settings,
            algorithm_params,
        )?;

        let handle = strategy.start().await;
        let strategy_id = strategy.id;

        let strategy_info = strategy.info().await;

        self.signal_manager
            .lock()
            .await
            .add_strategy_settings(strategy_id, strategy.settings());

        self.strategy_handles.insert(strategy.id, handle);
        self.strategies.insert(strategy.id, strategy);

        Ok(strategy_info)
    }

    pub async fn stop_strategy(
        &mut self,
        strategy_id: StrategyId,
        close_positions: bool,
    ) -> Option<StrategySummary> {
        let mut summary: Option<StrategySummary> = None;
        let account = self.account.clone();

        // Remove strategy handles
        if let Some(handle) = self.strategy_handles.get(&strategy_id) {
            handle.abort();

            // Call stop on strategy to update strategy internal state
            if let Some(strategy) = self.get_strategy(strategy_id) {
                let _summary = strategy.stop(account, close_positions).await;

                // Save summary
                self.storage_manager
                    .save_strategy_summary(_summary.clone())
                    .ok();

                summary = Some(_summary);
            }

            // Remove all handles and settings from signal_manager
            self.strategy_handles.remove(&strategy_id);
            self.strategies.remove(&strategy_id);
            self.signal_manager
                .lock()
                .await
                .remove_strategy_settings(strategy_id);
        };

        summary
    }

    pub fn get_active_strategy_ids(&mut self) -> Vec<StrategyId> {
        let mut strategies = vec![];
        for (strategy_id, _strategy) in self.strategies.iter() {
            strategies.push(*strategy_id)
        }

        strategies
    }

    pub fn get_strategy(&mut self, strategy_id: StrategyId) -> Option<&mut Strategy> {
        self.strategies.get_mut(&strategy_id)
    }

    pub fn list_historical_strategies(&mut self) -> Option<Vec<StrategySummary>> {
        self.storage_manager
            .list_all_saved_strategy_summaries()
            .ok()
    }

    pub fn get_historical_strategy_summary(
        &mut self,
        strategy_id: StrategyId,
    ) -> Option<StrategySummary> {
        self.storage_manager.get_strategy_summary(strategy_id).ok()
    }

    pub async fn run_back_test(
        &mut self,
        strategy_name: &str,
        symbol: &str,
        interval: &str,
        from_ts: u64,
        to_ts: u64,
        settings: StrategySettings,
        algorithm_params: Value,
    ) -> Result<StrategySummary, AlgorithmError> {
        let strategy_tx = self.strategy_tx.clone();
        let strategy = Strategy::new(
            strategy_name,
            symbol,
            interval,
            strategy_tx,
            self.market.clone(),
            settings,
            algorithm_params,
        )?;

        // TODO: Get initial_balance from params
        let initial_balance = Some(10_000.0);
        let mut back_test = BackTest::new(strategy, initial_balance).await;

        if let Some(kline_data) = self
            .market
            .clone()
            .lock()
            .await
            .kline_data_range(&symbol, &interval, Some(from_ts), Some(to_ts), None)
            .await
        {
            back_test.run(kline_data).await;
        };

        Ok(back_test.result().await)
    }

    // ---
    // Private Methods
    // ---

    async fn init(&mut self) {
        let signal_manager = self.signal_manager.clone();
        let strategy_rx = self.strategy_rx.clone();

        tokio::spawn(async move {
            while let Some(signal) = strategy_rx.lock().await.recv().await {
                signal_manager.lock().await.handle_signal(signal).await;
            }
        });
    }
}
