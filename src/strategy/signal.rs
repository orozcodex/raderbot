use std::{collections::HashMap, sync::Arc};

use log::info;

use crate::{
    account::{
        account::Account,
        trade::{OrderSide, Position},
    },
    market::{market::Market, types::ArcMutex},
};

use super::{
    strategy::{StrategyId, StrategySettings},
    types::SignalMessage,
};

pub struct SignalManager {
    account: ArcMutex<Account>,
    market: ArcMutex<Market>,
    active_strategy_settings: HashMap<StrategyId, StrategySettings>,
}

impl SignalManager {
    pub fn new(account: ArcMutex<Account>, market: ArcMutex<Market>) -> Self {
        Self {
            account,
            market,
            active_strategy_settings: HashMap::new(),
        }
    }

    pub async fn handle_signal(&mut self, signal: SignalMessage) {
        let active_positions = self
            .account
            .lock()
            .await
            .strategy_open_positions(signal.strategy_id)
            .await;

        // get trigger price used in all account actions
        // from market or signal if signal.is_back_test
        let trigger_price = if signal.is_back_test {
            Some(signal.price)
        } else {
            self.market.lock().await.last_price(&signal.symbol).await
        };

        if self
            .active_strategy_settings
            .get(&signal.strategy_id)
            .is_none()
        {
            return;
        }

        // SAFETY: None check above, used to make method more clear
        let settings = self
            .active_strategy_settings
            .get(&signal.strategy_id)
            .unwrap();

        // get last open position
        if let Some(last) = active_positions.last() {
            // if last.signal is different to new signal then close all positions
            if signal.order_side != last.order_side {
                if let Some(close_price) = trigger_price {
                    for position in &active_positions {
                        self.account
                            .lock()
                            .await
                            .close_position(position.id, close_price)
                            .await;
                    }
                }
            }

            // if is same signal as last position and settings allow more than one
            // open position
            if signal.order_side == last.order_side
                && active_positions.len() < settings.max_open_orders as usize
            {
                if let Some(close_price) = trigger_price {
                    self.account
                        .lock()
                        .await
                        .open_position(
                            &signal.symbol,
                            settings.margin_usd,
                            settings.leverage,
                            signal.order_side.clone(),
                            None,
                            close_price,
                        )
                        .await;
                }
            }
        } else {
            // no open positions yet for given strategy
            if let Some(last_price) = trigger_price {
                self.account
                    .lock()
                    .await
                    .open_position(
                        &signal.symbol,
                        settings.margin_usd,
                        settings.leverage,
                        signal.order_side.clone(),
                        None,
                        last_price,
                    )
                    .await;
            }
        }

        info!("{signal:?}");
    }

    pub fn add_strategy_settings(&mut self, strategy_id: u32, settings: StrategySettings) {
        self.active_strategy_settings.insert(strategy_id, settings);
    }

    pub fn remove_strategy_settings(&mut self, strategy_id: u32) {
        self.active_strategy_settings.remove(&strategy_id);
    }
}
