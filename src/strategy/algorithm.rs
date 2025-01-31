use std::time::Duration;

use serde_json::Value;

use crate::{
    algorithm::{
        bollinger_bands::BollingerBands, ma_crossover::EmaSmaCrossover,
        ma_simple::SimpleMovingAverage, ma_three_crossover::ThreeMaCrossover, macd::Macd,
        macd_bollinger::MacdBollingerBands, rsi::Rsi,
    },
    market::kline::Kline,
    utils::time::build_interval,
};

use super::types::{AlgorithmError, AlgorithmEvalResult};

/// Defines a trait for algorithm implementations used in trading strategies.
///
/// This trait outlines the necessary functionality for any algorithm used to evaluate trading
/// signals based on historical k-line (candlestick) data. It includes methods for evaluating  
/// trading signals, setting and retrieving algorithm parameters, and managing historical data
/// points.

pub trait Algorithm: Send + Sync {
    /// Evaluates a single k-line (candlestick) data point to generate a trading signal.
    ///
    /// # Arguments
    ///
    /// * `kline` - A `Kline` struct representing the k-line data to evaluate.
    ///
    /// # Returns
    ///
    /// An `AlgorithmEvalResult` indicating the trading signal generated by the algorithm.

    fn evaluate(&mut self, kline: Kline) -> AlgorithmEvalResult;

    /// Returns the time interval that the algorithm operates on.
    ///
    /// # Returns
    ///
    /// A `Duration` representing the interval between k-lines that the algorithm evaluates.

    fn interval(&self) -> Duration;

    /// Sets the algorithm's parameters based on a JSON `Value`.
    ///
    /// # Arguments
    ///
    /// * `params` - A `Value` containing the algorithm's configuration parameters.
    ///
    /// # Returns
    ///
    /// A `Result` indicating whether the parameters were successfully set or an error occurred.

    fn set_params(&mut self, params: Value) -> Result<(), AlgorithmError>;

    /// Retrieves the current parameters of the algorithm.
    ///
    /// # Returns
    ///
    /// A reference to a JSON `Value` containing the algorithm's current configuration parameters.

    fn get_params(&self) -> &Value;

    /// Provides access to the historical k-line data points the algorithm has evaluated.
    ///
    /// # Returns
    ///
    /// A vector of `Kline` structs representing the historical data points.

    // TODO: Create AlgorithmDataPointManager to handle data points
    // It will manage cleaning of data if data points length is too long,
    // to manage memory more efficiently as also prevent any bugs creeping
    // up that could occur when implementing a custom algorithm
    fn data_points(&self) -> Vec<Kline>;

    /// Cleans historical data points to manage memory usage efficiently.

    fn clean_data_points(&mut self);
}

/// A builder for constructing instances of algorithms based on their names and parameters.
///
/// This struct provides a method to build various trading algorithm instances dynamically
/// based on the algorithm's name, the desired interval for evaluation, and any specific
/// parameters required by the algorithm.

pub struct AlgorithmBuilder {}

impl AlgorithmBuilder {
    /// Constructs a new algorithm instance based on provided specifications.
    ///
    /// # Arguments
    ///
    /// * `algorithm_name` - A string slice representing the name of the algorithm to construct.
    /// * `interval` - A string slice representing the interval between k-lines for the algorithm's operation.
    /// * `algorithm_params` - A `Value` containing any specific parameters required by the algorithm.
    ///
    /// # Returns
    ///
    /// A `Result` containing the constructed algorithm boxed as a `dyn Algorithm` if successful,
    /// or an `AlgorithmError` if an error occurs during construction.

    pub fn build_algorithm(
        algorithm_name: &str,
        interval: &str,
        algorithm_params: Value,
    ) -> Result<Box<dyn Algorithm>, AlgorithmError> {
        let interval = match build_interval(interval) {
            Ok(interval) => interval,
            Err(e) => return Err(AlgorithmError::UnknownInterval(e.to_string())),
        };
        match algorithm_name {
            "EmaSmaCrossover" => {
                let algo = EmaSmaCrossover::new(interval, algorithm_params)?;
                Ok(Box::new(algo))
            }
            "SimpleMovingAverage" => {
                let algo = SimpleMovingAverage::new(interval, algorithm_params)?;
                Ok(Box::new(algo))
            }
            "ThreeMaCrossover" => {
                let algo = ThreeMaCrossover::new(interval, algorithm_params)?;
                Ok(Box::new(algo))
            }
            "Rsi" => {
                let algo = Rsi::new(interval, algorithm_params)?;
                Ok(Box::new(algo))
            }
            "RsiEmaSma" => {
                let algo = Rsi::new(interval, algorithm_params)?;
                Ok(Box::new(algo))
            }
            "BollingerBands" => {
                let algo = BollingerBands::new(interval, algorithm_params)?;
                Ok(Box::new(algo))
            }
            "Macd" => {
                let algo = Macd::new(interval, algorithm_params)?;
                Ok(Box::new(algo))
            }
            "MacdBollingerBands" => {
                let algo = MacdBollingerBands::new(interval, algorithm_params)?;
                Ok(Box::new(algo))
            }
            _ => Err(AlgorithmError::UnkownName(
                format!("Strategy name {algorithm_name} is incorrect").to_string(),
            )),
        }
    }
}
