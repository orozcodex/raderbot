use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use log::info;
use serde_json::Value;

use crate::account::account::Account;
use crate::account::trade::{OrderSide, PositionId, TradeTx};
use crate::exchange::api::ExchangeApi;
use crate::exchange::mock::MockExchangeApi;
use crate::market::kline::KlineData;
use crate::market::market::Market;
use crate::market::messages::MarketMessage;
use crate::market::types::ArcMutex;
use crate::storage::fs::FsStorageManager;
use crate::utils::channel::build_arc_channel;

use super::signal::SignalManager;
use super::strategy::{Strategy, StrategyResult};
use super::types::{AlgorithmEvalResult, SignalMessage};

pub struct BackTest {
    pub strategy: Strategy,
    pub signals: Vec<SignalMessage>,
    pub signal_manager: SignalManager,
    account: ArcMutex<Account>,
    period_start_price: f64,
    period_end_price: f64,
}

impl BackTest {
    pub async fn new(strategy: Strategy, initial_balance: Option<f64>) -> Self {
        let (_, market_rx) = build_arc_channel::<MarketMessage>();
        let exchange_api: Arc<Box<dyn ExchangeApi>> =
            Arc::new(Box::new(MockExchangeApi::default()));

        let storage_manager = Box::new(FsStorageManager::default());

        let market = ArcMutex::new(
            Market::new(market_rx, exchange_api.clone(), storage_manager, false).await,
        );

        // create new storage manager
        let account = ArcMutex::new(Account::new(exchange_api.clone(), false, true).await);

        let mut signal_manager = SignalManager::new(account.clone(), market.clone());
        signal_manager.add_strategy_settings(strategy.id, strategy.settings());

        Self {
            strategy,
            signals: vec![],
            signal_manager,
            account,
            period_end_price: 0.0,
            period_start_price: 0.0,
        }
    }

    pub async fn run(&mut self, kline_data: KlineData) {
        if let Some(first) = kline_data.klines.first() {
            self.period_start_price = first.open
        }
        if let Some(last) = kline_data.klines.last() {
            self.period_end_price = last.close
        }

        for kline in kline_data.klines {
            let eval_result = self.strategy.algorithm.lock().await.evaluate(kline.clone());

            let order_side = match eval_result {
                AlgorithmEvalResult::Long => OrderSide::Long,
                AlgorithmEvalResult::Short => OrderSide::Short,
                AlgorithmEvalResult::Ignore => {
                    continue;
                }
            };

            let signal = SignalMessage {
                strategy_id: self.strategy.id,
                order_side,
                symbol: self.strategy.symbol.to_string(),
                price: kline.close.clone(),
                is_back_test: true,
                timestamp: kline.close_time,
            };

            self.add_signal(signal)
        }
    }

    pub fn add_signal(&mut self, signal: SignalMessage) {
        self.signals.push(signal)
    }

    pub fn calc_max_profit(&self, trades: &Vec<TradeTx>) -> f64 {
        let mut max_balance = 0.0;
        let mut current_balance = 0.0;

        for trade_tx in trades {
            let profit = trade_tx.calc_profit();
            current_balance += profit;

            if current_balance > max_balance {
                max_balance = current_balance;
            }
        }

        max_balance
    }

    pub fn calc_max_drawdown(&self, trades: &Vec<TradeTx>) -> f64 {
        let mut min_balance = f64::MAX;
        let mut current_balance = 0.0;

        for trade_tx in trades {
            let profit = trade_tx.calc_profit();
            current_balance += profit;

            if current_balance < min_balance {
                min_balance = current_balance;
            }
        }

        min_balance
    }

    pub async fn result(&mut self) -> StrategyResult {
        for signal in &self.signals {
            self.signal_manager.handle_signal(signal.clone()).await
        }

        let active_positions: Vec<(PositionId, f64)> = self
            .account
            .lock()
            .await
            .positions()
            .into_iter()
            .map(|item| (item.id, item.open_price))
            .collect();

        // close any remaining positions
        for (id, open_price) in active_positions {
            self.account
                .lock()
                .await
                .close_position(id, open_price)
                .await;
        }

        // get all trade txs
        let trades: Vec<TradeTx> = self.account.lock().await.trades();

        let max_profit = self.calc_max_profit(&trades);
        let max_drawdown = self.calc_max_drawdown(&trades);

        let profit: f64 = trades.iter().map(|trade| trade.calc_profit()).sum();
        let long_count = trades
            .iter()
            .filter(|trade| trade.position.order_side == OrderSide::Long)
            .count();
        let short_count = trades.len() - long_count;

        StrategyResult {
            profit,
            // trades,
            long_count,
            short_count,
            // signals,
            // // buy_signal_count,
            // sell_signal_count,
            symbol: self.strategy.symbol.to_string(),
            period_end_price: self.period_end_price,
            period_start_price: self.period_start_price,
            max_drawdown,
            max_profit,
        }
    }
}
