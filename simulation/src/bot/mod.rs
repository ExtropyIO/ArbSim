//! Bot interface for the simulation framework.
//!
//! Bots implement the `ArbitrageBot` trait to participate in simulations.
//! The trait is designed for both testing and competitive arenas.

use crate::{BotAction, SimBlock, SimState, SimSwap};
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait for arbitrage bots participating in simulations.
///
/// Bots receive events about the simulation state and can respond with actions.
/// The simulation engine will execute valid actions and track metrics.
///
/// # Example
///
/// ```text
/// use sneak_simulation::bot::ArbitrageBot;
///
/// struct MyBot {
///     id: String,
/// }
///
/// #[async_trait]
/// impl ArbitrageBot for MyBot {
///     fn id(&self) -> &str {
///         &self.id
///     }
///
///     async fn on_block_start(&mut self, block: &SimBlock, state: &SimState) {
///         // Analyze state at block start
///     }
///
///     async fn on_swap(&mut self, swap: &SimSwap, state: &SimState) -> Option<BotAction> {
///         // React to swaps - look for arbitrage opportunities
///         None
///     }
/// }
/// ```
#[async_trait]
pub trait ArbitrageBot: Send + Sync {
    /// Returns the unique identifier for this bot
    fn id(&self) -> &str;

    /// Returns the bot's display name
    fn name(&self) -> &str {
        self.id()
    }

    /// Called when a new block starts.
    /// Use this to analyze the current state and prepare for the block.
    async fn on_block_start(&mut self, block: &SimBlock, state: &SimState);

    /// Called when a swap occurs in the simulation.
    /// Return a `BotAction` to respond, or `None` to do nothing.
    async fn on_swap(&mut self, swap: &SimSwap, state: &SimState) -> Option<BotAction>;

    /// Called when the block ends.
    /// Use this for cleanup or analysis.
    async fn on_block_end(&mut self, _block: &SimBlock, _state: &SimState) {
        // Default: do nothing
    }

    /// Called when the simulation ends.
    /// Return any final statistics or data.
    async fn on_simulation_end(&mut self, _state: &SimState) -> Option<BotStats> {
        None
    }

    /// Get the current balance of a token for this bot.
    /// The simulation engine manages balances, but bots can query them.
    fn get_balance(&self, _token: &alloy_primitives::Address) -> alloy_primitives::U256 {
        alloy_primitives::U256::ZERO
    }
}

/// Statistics reported by a bot at the end of simulation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BotStats {
    /// Custom statistics
    pub custom: HashMap<String, Decimal>,
    /// Number of opportunities found
    pub opportunities_found: u64,
    /// Number of trades executed
    pub trades_executed: u64,
    /// Number of failed trades
    pub trades_failed: u64,
    /// Any notes or observations
    pub notes: Vec<String>,
}

impl BotStats {
    /// Create new empty stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a custom statistic
    pub fn with_stat(mut self, key: impl Into<String>, value: Decimal) -> Self {
        self.custom.insert(key.into(), value);
        self
    }

    /// Add a note
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// Bot registration info
#[derive(Debug, Clone)]
pub struct BotInfo {
    /// Bot ID
    pub id: String,
    /// Bot display name
    pub name: String,
    /// Starting balance (per token, in human-readable units)
    pub starting_balance: HashMap<String, Decimal>,
}

impl BotInfo {
    /// Create new bot info
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            starting_balance: HashMap::new(),
        }
    }

    /// Set starting balance for a token
    pub fn with_balance(mut self, token_symbol: impl Into<String>, amount: Decimal) -> Self {
        self.starting_balance.insert(token_symbol.into(), amount);
        self
    }
}

/// A simple pass-through bot that does nothing (for testing)
pub struct PassiveBot {
    id: String,
}

impl PassiveBot {
    /// Create a new passive bot
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl ArbitrageBot for PassiveBot {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "Passive Bot"
    }

    async fn on_block_start(&mut self, _block: &SimBlock, _state: &SimState) {
        // Do nothing
    }

    async fn on_swap(&mut self, _swap: &SimSwap, _state: &SimState) -> Option<BotAction> {
        None // Never act
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChainId;
    use alloy_primitives::{Address, U256};

    #[tokio::test]
    async fn test_passive_bot() {
        let mut bot = PassiveBot::new("test");
        assert_eq!(bot.id(), "test");
        assert_eq!(bot.name(), "Passive Bot");

        let state = SimState::new(ChainId::Arbitrum);
        let block = SimBlock::new(1, U256::from(100), 30_000_000);

        bot.on_block_start(&block, &state).await;

        let swap = SimSwap::new(
            Address::ZERO,
            Address::ZERO,
            Address::ZERO,
            Address::ZERO,
            U256::ZERO,
            U256::ZERO,
            1,
        );

        let action = bot.on_swap(&swap, &state).await;
        assert!(action.is_none());
    }

    #[test]
    fn test_bot_stats() {
        let stats = BotStats::new()
            .with_stat("profit_usd", Decimal::from(100))
            .with_note("Good performance");

        assert_eq!(stats.custom.get("profit_usd"), Some(&Decimal::from(100)));
        assert_eq!(stats.notes.len(), 1);
    }

    #[test]
    fn test_bot_info() {
        let info = BotInfo::new("bot1", "My Bot")
            .with_balance("USDC", Decimal::from(10000))
            .with_balance("WETH", Decimal::from(10));

        assert_eq!(info.id, "bot1");
        assert_eq!(info.name, "My Bot");
        assert_eq!(info.starting_balance.get("USDC"), Some(&Decimal::from(10000)));
    }
}
