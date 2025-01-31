use crate::market::kline::Kline;
use crate::strategy::types::AlgorithmError;
use crate::strategy::{algorithm::Algorithm, types::AlgorithmEvalResult};
use crate::utils::number::parse_usize_from_value;
use serde_json::Value;
use std::time::Duration;

pub struct RsiEmaSma {
    data_points: Vec<Kline>,
    interval: Duration,
    params: Value,
    rsi_period: usize,
    short_sma_period: usize,
    medium_sma_period: usize,
    long_sma_period: usize,
    ema_period: usize,
    last_ema: f64, // Stores the last EMA value for incremental calculation
}

impl RsiEmaSma {
    pub fn new(interval: Duration, params: Value) -> Result<Self, AlgorithmError> {
        let rsi_period = parse_usize_from_value("rsi_period", &params).unwrap_or(14);
        let short_sma_period = parse_usize_from_value("short_sma_period", &params).unwrap_or(5);
        let medium_sma_period = parse_usize_from_value("medium_sma_period", &params).unwrap_or(12);
        let long_sma_period = parse_usize_from_value("long_sma_period", &params).unwrap_or(26);
        let ema_period = parse_usize_from_value("ema_period", &params).unwrap_or(9);

        Ok(Self {
            data_points: Vec::new(),
            interval,
            params,
            rsi_period,
            short_sma_period,
            medium_sma_period,
            long_sma_period,
            ema_period,
            last_ema: 0.0,
        })
    }

    fn calculate_rsi(&self) -> f64 {
        // Simplified RSI calculation, assumes calculate_gain_loss function is defined
        if self.data_points.len() < self.rsi_period + 1 {
            return 50.0; // Default RSI value if not enough data
        }

        let mut gains = 0.0;
        let mut losses = 0.0;
        for i in (1..=self.rsi_period).rev() {
            let delta = self.data_points[self.data_points.len() - i].close
                - self.data_points[self.data_points.len() - i - 1].close;
            if delta > 0.0 {
                gains += delta;
            } else {
                losses -= delta;
            }
        }

        let avg_gain = gains / self.rsi_period as f64;
        let avg_loss = losses / self.rsi_period as f64;

        if avg_loss == 0.0 {
            return 100.0;
        }

        let rs = avg_gain / avg_loss;
        100.0 - (100.0 / (1.0 + rs))
    }

    fn calculate_sma(&self, period: usize) -> f64 {
        if self.data_points.len() < period {
            return 0.0; // Not enough data
        }
        self.data_points
            .iter()
            .rev()
            .take(period)
            .map(|k| k.close)
            .sum::<f64>()
            / period as f64
    }

    fn calculate_ema(&mut self, period: usize) -> f64 {
        if self.data_points.is_empty() {
            return 0.0;
        }

        let k = 2.0 / (period as f64 + 1.0);
        let close_price = self.data_points.last().unwrap().close;

        if self.last_ema == 0.0 {
            // First calculation
            self.last_ema = close_price;
        } else {
            self.last_ema = (close_price - self.last_ema) * k + self.last_ema;
        }

        self.last_ema
    }
}

impl Algorithm for RsiEmaSma {
    fn evaluate(&mut self, kline: Kline) -> AlgorithmEvalResult {
        self.data_points.push(kline);

        let rsi = self.calculate_rsi();
        let short_sma = self.calculate_sma(self.short_sma_period);
        let medium_sma = self.calculate_sma(self.medium_sma_period);
        let long_sma = self.calculate_sma(self.long_sma_period);
        let ema = self.calculate_ema(self.ema_period);

        let result = if rsi < 30.0
            && short_sma > medium_sma
            && medium_sma > long_sma
            && short_sma > ema
        {
            AlgorithmEvalResult::Buy
        } else if rsi > 70.0 && short_sma < medium_sma && medium_sma < long_sma && short_sma < ema {
            AlgorithmEvalResult::Sell
        } else {
            AlgorithmEvalResult::Ignore
        };

        self.clean_data_points();

        result
    }

    // Implement the rest of the required methods from the Algorithm trait...
    fn data_points(&self) -> Vec<Kline> {
        self.data_points.clone()
    }

    fn interval(&self) -> Duration {
        self.interval
    }

    fn get_params(&self) -> &Value {
        &self.params
    }

    fn set_params(&mut self, params: Value) -> Result<(), AlgorithmError> {
        let rsi_period = parse_usize_from_value("rsi_period", &params).unwrap_or(self.rsi_period); // Default to 14 if not specified

        let short_sma_period =
            parse_usize_from_value("short_sma_period", &params).unwrap_or(self.short_sma_period);
        let medium_sma_period =
            parse_usize_from_value("medium_sma_period", &params).unwrap_or(self.medium_sma_period);
        let long_sma_period =
            parse_usize_from_value("long_sma_period", &params).unwrap_or(self.long_sma_period);
        let ema_period = parse_usize_from_value("ema_period", &params).unwrap_or(9);

        self.rsi_period = rsi_period;
        self.short_sma_period = short_sma_period;
        self.medium_sma_period = medium_sma_period;
        self.long_sma_period = long_sma_period;
        self.ema_period = ema_period;
        self.params = params;

        Ok(())
    }

    fn clean_data_points(&mut self) {
        // TODO: Change length to be checked
        // based on individual algorithm
        let two_weeks_minutes = 10080 * 2;
        if self.data_points.len() > two_weeks_minutes {
            // reduce back to 1 week worth on data
            self.data_points.drain(0..10080);
        }
    }
}
