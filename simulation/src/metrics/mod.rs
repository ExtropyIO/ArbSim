//! Metrics collection for bot performance analysis.
//!
//! Tracks various metrics for each bot participating in the simulation,
//! enabling performance comparison and analysis.

use crate::{BotAction, SimSwap};
use alloy_primitives::U256;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metrics for a single bot
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BotMetrics {
    /// Bot identifier
    pub bot_id: String,
    /// Total profit in USD
    pub total_profit_usd: Decimal,
    /// Total gas spent in native token
    pub total_gas_spent: U256,
    /// Number of trades attempted
    pub trades_attempted: u64,
    /// Number of trades successfully executed
    pub trades_successful: u64,
    /// Number of trades that failed
    pub trades_failed: u64,
    /// Total volume traded (input amount)
    pub total_volume: U256,
    /// Average latency (time from event to action)
    pub avg_latency_ms: f64,
    /// Maximum drawdown observed
    pub max_drawdown_usd: Decimal,
    /// Win rate (successful / attempted)
    pub win_rate: f64,
    /// Profit factor (gross profit / gross loss)
    pub profit_factor: f64,
    /// Sharpe ratio (if enough data)
    pub sharpe_ratio: Option<f64>,
    /// Starting balance
    pub starting_balance_usd: Decimal,
    /// Ending balance
    pub ending_balance_usd: Decimal,
    /// ROI percentage
    pub roi_percent: Decimal,
    /// Trade history
    pub trades: Vec<TradeRecord>,
    /// Profit/loss history for analysis
    pnl_history: Vec<Decimal>,
    /// Peak balance for drawdown calculation
    peak_balance: Decimal,
}

impl BotMetrics {
    /// Create new metrics for a bot
    pub fn new(bot_id: impl Into<String>, starting_balance_usd: Decimal) -> Self {
        Self {
            bot_id: bot_id.into(),
            starting_balance_usd,
            ending_balance_usd: starting_balance_usd,
            peak_balance: starting_balance_usd,
            ..Default::default()
        }
    }

    /// Record a trade attempt
    pub fn record_trade(&mut self, trade: TradeRecord) {
        self.trades_attempted += 1;

        if trade.success {
            self.trades_successful += 1;
            self.total_profit_usd += trade.profit_usd;
            self.total_volume += trade.amount_in;
            self.total_gas_spent += trade.gas_used;

            // Update balance
            self.ending_balance_usd += trade.profit_usd;

            // Update peak and drawdown
            if self.ending_balance_usd > self.peak_balance {
                self.peak_balance = self.ending_balance_usd;
            } else {
                let drawdown = self.peak_balance - self.ending_balance_usd;
                if drawdown > self.max_drawdown_usd {
                    self.max_drawdown_usd = drawdown;
                }
            }

            // Track P&L for Sharpe calculation
            self.pnl_history.push(trade.profit_usd);
        } else {
            self.trades_failed += 1;
            // Gas is still spent on failed trades
            self.total_gas_spent += trade.gas_used;
        }

        self.trades.push(trade);

        // Recalculate derived metrics
        self.update_derived_metrics();
    }

    /// Update derived metrics (win rate, ROI, etc.)
    fn update_derived_metrics(&mut self) {
        // Win rate
        if self.trades_attempted > 0 {
            self.win_rate = self.trades_successful as f64 / self.trades_attempted as f64;
        }

        // ROI
        if self.starting_balance_usd > Decimal::ZERO {
            self.roi_percent = ((self.ending_balance_usd - self.starting_balance_usd)
                / self.starting_balance_usd)
                * dec!(100);
        }

        // Profit factor
        let gross_profit: Decimal = self.pnl_history
            .iter()
            .filter(|&&p| p > Decimal::ZERO)
            .sum();
        let gross_loss: Decimal = self.pnl_history
            .iter()
            .filter(|&&p| p < Decimal::ZERO)
            .map(|p| p.abs())
            .sum();

        if gross_loss > Decimal::ZERO {
            self.profit_factor = (gross_profit / gross_loss).to_string().parse().unwrap_or(0.0);
        } else if gross_profit > Decimal::ZERO {
            self.profit_factor = f64::INFINITY;
        }

        // Sharpe ratio (simplified - assumes risk-free rate of 0)
        if self.pnl_history.len() >= 10 {
            let returns: Vec<f64> = self.pnl_history
                .iter()
                .map(|d| d.to_string().parse().unwrap_or(0.0))
                .collect();

            let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
            let variance: f64 = returns.iter()
                .map(|r| (r - mean).powi(2))
                .sum::<f64>() / returns.len() as f64;
            let std_dev = variance.sqrt();

            if std_dev > 0.0 {
                self.sharpe_ratio = Some(mean / std_dev);
            }
        }
    }

    /// Get summary statistics
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            bot_id: self.bot_id.clone(),
            total_profit_usd: self.total_profit_usd,
            trades_executed: self.trades_successful,
            win_rate: self.win_rate,
            roi_percent: self.roi_percent,
            max_drawdown_usd: self.max_drawdown_usd,
            sharpe_ratio: self.sharpe_ratio,
        }
    }
}

/// Record of a single trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    /// Trade ID
    pub id: uuid::Uuid,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Block number
    pub block_number: u64,
    /// Whether the trade was successful
    pub success: bool,
    /// Pool address
    pub pool: alloy_primitives::Address,
    /// Input token
    pub token_in: alloy_primitives::Address,
    /// Output token
    pub token_out: alloy_primitives::Address,
    /// Input amount
    pub amount_in: U256,
    /// Output amount
    pub amount_out: U256,
    /// Gas used
    pub gas_used: U256,
    /// Profit/loss in USD
    pub profit_usd: Decimal,
    /// Latency from trigger event to execution (ms)
    pub latency_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
}

impl TradeRecord {
    /// Create a successful trade record
    pub fn success(
        swap: &SimSwap,
        profit_usd: Decimal,
        latency_ms: u64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            block_number: swap.block_number,
            success: true,
            pool: swap.pool,
            token_in: swap.token_in,
            token_out: swap.token_out,
            amount_in: swap.amount_in,
            amount_out: swap.amount_out,
            gas_used: U256::from(swap.gas_used),
            profit_usd,
            latency_ms,
            error: None,
        }
    }

    /// Create a failed trade record
    pub fn failure(
        action: &BotAction,
        error: impl Into<String>,
        gas_used: U256,
    ) -> Self {
        let (pool, token_in, token_out, amount_in) = if let Some(swap) = action.swaps.first() {
            (swap.pool, swap.token_in, swap.token_out, swap.amount_in)
        } else {
            (alloy_primitives::Address::ZERO, alloy_primitives::Address::ZERO, alloy_primitives::Address::ZERO, U256::ZERO)
        };

        Self {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            block_number: 0,
            success: false,
            pool,
            token_in,
            token_out,
            amount_in,
            amount_out: U256::ZERO,
            gas_used,
            profit_usd: Decimal::ZERO,
            latency_ms: 0,
            error: Some(error.into()),
        }
    }
}

/// Summary of metrics for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub bot_id: String,
    pub total_profit_usd: Decimal,
    pub trades_executed: u64,
    pub win_rate: f64,
    pub roi_percent: Decimal,
    pub max_drawdown_usd: Decimal,
    pub sharpe_ratio: Option<f64>,
}

/// Collector for aggregating metrics from all bots
pub struct MetricsCollector {
    bot_metrics: HashMap<String, BotMetrics>,
    simulation_start: DateTime<Utc>,
    blocks_processed: u64,
    total_swaps: u64,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            bot_metrics: HashMap::new(),
            simulation_start: Utc::now(),
            blocks_processed: 0,
            total_swaps: 0,
        }
    }

    /// Register a bot for tracking
    pub fn register_bot(&mut self, bot_id: impl Into<String>, starting_balance_usd: Decimal) {
        let id = bot_id.into();
        self.bot_metrics.insert(id.clone(), BotMetrics::new(id, starting_balance_usd));
    }

    /// Record a trade for a bot
    pub fn record_trade(&mut self, bot_id: &str, trade: TradeRecord) {
        if let Some(metrics) = self.bot_metrics.get_mut(bot_id) {
            metrics.record_trade(trade);
        }
    }

    /// Record a block processed
    pub fn record_block(&mut self, num_swaps: u64) {
        self.blocks_processed += 1;
        self.total_swaps += num_swaps;
    }

    /// Get metrics for a specific bot
    pub fn get_bot_metrics(&self, bot_id: &str) -> Option<&BotMetrics> {
        self.bot_metrics.get(bot_id)
    }

    /// Get all bot metrics
    pub fn get_all_metrics(&self) -> &HashMap<String, BotMetrics> {
        &self.bot_metrics
    }

    /// Get leaderboard (sorted by profit)
    pub fn leaderboard(&self) -> Vec<MetricsSummary> {
        let mut summaries: Vec<_> = self.bot_metrics
            .values()
            .map(|m| m.summary())
            .collect();

        summaries.sort_by(|a, b| b.total_profit_usd.cmp(&a.total_profit_usd));
        summaries
    }

    /// Get simulation duration
    pub fn duration(&self) -> Duration {
        Utc::now() - self.simulation_start
    }

    /// Get total blocks processed
    pub fn blocks_processed(&self) -> u64 {
        self.blocks_processed
    }

    /// Get total swaps processed
    pub fn total_swaps(&self) -> u64 {
        self.total_swaps
    }

    /// Generate a report
    pub fn report(&self) -> SimulationReport {
        SimulationReport {
            duration_ms: self.duration().num_milliseconds(),
            blocks_processed: self.blocks_processed,
            total_swaps: self.total_swaps,
            leaderboard: self.leaderboard(),
            bot_metrics: self.bot_metrics.values().cloned().collect(),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Complete simulation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationReport {
    /// Simulation duration in milliseconds
    pub duration_ms: i64,
    /// Blocks processed
    pub blocks_processed: u64,
    /// Total swaps
    pub total_swaps: u64,
    /// Leaderboard
    pub leaderboard: Vec<MetricsSummary>,
    /// Detailed metrics per bot
    pub bot_metrics: Vec<BotMetrics>,
}

impl SimulationReport {
    /// Export to JSON
    pub fn to_json(&self) -> crate::Result<String> {
        serde_json::to_string_pretty(self).map_err(crate::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;

    #[test]
    fn test_bot_metrics_new() {
        let metrics = BotMetrics::new("test_bot", dec!(10000));
        assert_eq!(metrics.bot_id, "test_bot");
        assert_eq!(metrics.starting_balance_usd, dec!(10000));
        assert_eq!(metrics.ending_balance_usd, dec!(10000));
    }

    #[test]
    fn test_record_successful_trade() {
        let mut metrics = BotMetrics::new("test_bot", dec!(10000));

        let trade = TradeRecord {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            block_number: 100,
            success: true,
            pool: Address::ZERO,
            token_in: Address::ZERO,
            token_out: Address::ZERO,
            amount_in: U256::from(1000u64),
            amount_out: U256::from(1100u64),
            gas_used: U256::from(150000u64),
            profit_usd: dec!(50),
            latency_ms: 10,
            error: None,
        };

        metrics.record_trade(trade);

        assert_eq!(metrics.trades_attempted, 1);
        assert_eq!(metrics.trades_successful, 1);
        assert_eq!(metrics.total_profit_usd, dec!(50));
        assert_eq!(metrics.win_rate, 1.0);
    }

    #[test]
    fn test_win_rate_calculation() {
        let mut metrics = BotMetrics::new("test_bot", dec!(10000));

        // Record 3 successful, 1 failed
        for i in 0..4 {
            let trade = TradeRecord {
                id: uuid::Uuid::new_v4(),
                timestamp: Utc::now(),
                block_number: 100 + i,
                success: i < 3, // First 3 succeed
                pool: Address::ZERO,
                token_in: Address::ZERO,
                token_out: Address::ZERO,
                amount_in: U256::from(1000u64),
                amount_out: if i < 3 { U256::from(1100u64) } else { U256::ZERO },
                gas_used: U256::from(150000u64),
                profit_usd: if i < 3 { dec!(50) } else { Decimal::ZERO },
                latency_ms: 10,
                error: if i >= 3 { Some("Failed".into()) } else { None },
            };
            metrics.record_trade(trade);
        }

        assert_eq!(metrics.trades_attempted, 4);
        assert_eq!(metrics.trades_successful, 3);
        assert_eq!(metrics.win_rate, 0.75);
    }

    #[test]
    fn test_metrics_collector_leaderboard() {
        let mut collector = MetricsCollector::new();

        collector.register_bot("bot_a", dec!(10000));
        collector.register_bot("bot_b", dec!(10000));

        // Bot A makes $100 profit
        let trade_a = TradeRecord {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            block_number: 100,
            success: true,
            pool: Address::ZERO,
            token_in: Address::ZERO,
            token_out: Address::ZERO,
            amount_in: U256::from(1000u64),
            amount_out: U256::from(1100u64),
            gas_used: U256::from(150000u64),
            profit_usd: dec!(100),
            latency_ms: 10,
            error: None,
        };
        collector.record_trade("bot_a", trade_a);

        // Bot B makes $200 profit
        let trade_b = TradeRecord {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            block_number: 100,
            success: true,
            pool: Address::ZERO,
            token_in: Address::ZERO,
            token_out: Address::ZERO,
            amount_in: U256::from(1000u64),
            amount_out: U256::from(1200u64),
            gas_used: U256::from(150000u64),
            profit_usd: dec!(200),
            latency_ms: 10,
            error: None,
        };
        collector.record_trade("bot_b", trade_b);

        let leaderboard = collector.leaderboard();

        assert_eq!(leaderboard.len(), 2);
        assert_eq!(leaderboard[0].bot_id, "bot_b"); // Higher profit first
        assert_eq!(leaderboard[1].bot_id, "bot_a");
    }
}
