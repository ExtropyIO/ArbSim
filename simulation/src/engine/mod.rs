//! Core simulation engine.
//!
//! The `SimulationEngine` orchestrates the simulation by:
//! - Processing events in priority order
//! - Executing swaps through DEX adapters
//! - Notifying bots of market activity
//! - Collecting metrics

use crate::{
    bot::ArbitrageBot,
    dex::DexRegistry,
    event::{Event, EventQueue},
    metrics::{MetricsCollector, TradeRecord},
    scenario::{ScenarioConfig, ScenarioGenerator},
    state::SimState,
    types::*,
    Error, Result,
};
use alloy_primitives::{Address, U256};
use chrono::Utc;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Main simulation engine
pub struct SimulationEngine {
    /// Configuration
    config: SimConfig,
    /// Simulation state
    state: Arc<RwLock<SimState>>,
    /// Event queue
    events: EventQueue,
    /// DEX adapters
    dex_registry: DexRegistry,
    /// Registered bots
    bots: Vec<Box<dyn ArbitrageBot>>,
    /// Bot addresses for balance tracking
    bot_addresses: HashMap<String, Address>,
    /// Scenario generator
    scenario: ScenarioGenerator,
    /// Metrics collector
    metrics: MetricsCollector,
    /// Whether simulation is running
    running: bool,
    /// Current block
    current_block: u64,
}

impl SimulationEngine {
    /// Create a new simulation engine
    pub fn new(config: SimConfig) -> Self {
        let scenario_config = ScenarioConfig {
            seed: config.random_seed,
            ..Default::default()
        };

        let state = SimState::new(config.chain_id);

        Self {
            config,
            state: Arc::new(RwLock::new(state)),
            events: EventQueue::new(),
            dex_registry: DexRegistry::new(),
            bots: Vec::new(),
            bot_addresses: HashMap::new(),
            scenario: ScenarioGenerator::new(scenario_config),
            metrics: MetricsCollector::new(),
            running: false,
            current_block: 0,
        }
    }

    /// Create a new engine with custom scenario config
    pub fn with_scenario(config: SimConfig, scenario_config: ScenarioConfig) -> Self {
        let state = SimState::new(config.chain_id);

        Self {
            config,
            state: Arc::new(RwLock::new(state)),
            events: EventQueue::new(),
            dex_registry: DexRegistry::new(),
            bots: Vec::new(),
            bot_addresses: HashMap::new(),
            scenario: ScenarioGenerator::new(scenario_config),
            metrics: MetricsCollector::new(),
            running: false,
            current_block: 0,
        }
    }

    /// Register a bot with the simulation
    pub fn register_bot(
        &mut self,
        bot: Box<dyn ArbitrageBot>,
        starting_balance_usd: Decimal,
    ) -> Result<()> {
        let bot_id = bot.id().to_string();

        if self.bot_addresses.contains_key(&bot_id) {
            return Err(Error::BotRegistration(format!(
                "Bot '{}' is already registered",
                bot_id
            )));
        }

        // Generate a unique address for this bot
        let bot_address = Address::repeat_byte((self.bots.len() + 1) as u8 + 0xA0);

        self.bot_addresses.insert(bot_id.clone(), bot_address);
        self.metrics.register_bot(&bot_id, starting_balance_usd);
        self.bots.push(bot);

        info!(bot_id = %bot_id, address = %bot_address, "Registered bot");

        Ok(())
    }

    /// Add a pool to the simulation
    pub fn add_pool(&self, pool: SimPool) {
        self.state.write().set_pool(pool);
    }

    /// Add multiple pools
    pub fn add_pools(&self, pools: impl IntoIterator<Item = SimPool>) {
        let state = self.state.write();
        for pool in pools {
            state.set_pool(pool);
        }
    }

    /// Set balance for a bot
    pub fn set_bot_balance(&self, bot_id: &str, token: &Address, amount: U256) -> Result<()> {
        let bot_address = self.bot_addresses.get(bot_id)
            .ok_or_else(|| Error::BotNotFound(bot_id.to_string()))?;

        self.state.read().set_balance(bot_address, token, amount);
        Ok(())
    }

    /// Get reference to state
    pub fn state(&self) -> Arc<RwLock<SimState>> {
        Arc::clone(&self.state)
    }

    /// Get reference to metrics
    pub fn metrics(&self) -> &MetricsCollector {
        &self.metrics
    }

    /// Run the simulation for a specified number of blocks
    pub async fn run(&mut self, num_blocks: u64) -> Result<SimulationReport> {
        if self.running {
            return Err(Error::AlreadyRunning);
        }

        self.running = true;
        let start_block = self.config.start_block.max(1);
        let end_block = start_block + num_blocks;

        info!(
            start_block = start_block,
            end_block = end_block,
            num_bots = self.bots.len(),
            "Starting simulation"
        );

        for block_num in start_block..end_block {
            self.process_block(block_num).await?;

            // Real-time mode: sleep between blocks
            if self.config.real_time_mode {
                tokio::time::sleep(self.config.block_duration()).await;
            }
        }

        self.running = false;

        // Notify bots of simulation end
        let state = self.state.read();
        for bot in &mut self.bots {
            bot.on_simulation_end(&state).await;
        }

        info!(
            blocks_processed = self.metrics.blocks_processed(),
            total_swaps = self.metrics.total_swaps(),
            "Simulation complete"
        );

        Ok(self.metrics.report())
    }

    /// Process a single block
    async fn process_block(&mut self, block_number: u64) -> Result<()> {
        self.current_block = block_number;
        let block_start = Instant::now();

        // Update state for new block
        {
            let mut state = self.state.write();
            state.block_number = block_number;
            state.timestamp = Utc::now();
        }

        // Create block
        let block = SimBlock::new(
            block_number,
            self.config.base_gas_price,
            self.config.gas_limit,
        );

        // Generate market activity
        let market_events = {
            let state = self.state.read();
            self.scenario.generate_block_events(&state, block_number)
        };

        // Queue block start event
        self.events.push(Event::BlockStart {
            block_number,
            timestamp: Utc::now(),
        });

        // Queue market events
        self.events.push_all(market_events);

        // Queue block end event
        self.events.push(Event::BlockEnd { block_number });

        // Process all events for this block
        let mut swaps_in_block = 0;
        while let Some(event) = self.events.pop() {
            match event {
                Event::BlockStart { block_number, timestamp: _ } => {
                    debug!(block = block_number, "Block start");
                    self.notify_bots_block_start(&block).await;
                }

                Event::MarketSwap(swap) => {
                    if let Ok(executed_swap) = self.execute_market_swap(swap).await {
                        swaps_in_block += 1;
                        self.notify_bots_swap(&executed_swap).await;
                    }
                }

                Event::BotAction(action) => {
                    self.execute_bot_action(action).await;
                }

                Event::BlockEnd { block_number } => {
                    debug!(
                        block = block_number,
                        swaps = swaps_in_block,
                        elapsed_ms = block_start.elapsed().as_millis(),
                        "Block end"
                    );
                    self.notify_bots_block_end(&block).await;
                }

                Event::SimulationEnd => break,
            }
        }

        self.metrics.record_block(swaps_in_block);

        Ok(())
    }

    /// Execute a market-generated swap
    async fn execute_market_swap(&self, mut swap: SimSwap) -> Result<SimSwap> {
        let state = self.state.write();

        let pool = state.get_pool(&swap.pool)
            .ok_or_else(|| Error::PoolNotFound(swap.pool.to_string()))?;

        // Get the appropriate DEX adapter
        let adapter = self.dex_registry.get_for_pool(&pool)
            .ok_or_else(|| Error::UnsupportedDex(pool.dex_type.to_string()))?;

        // Calculate output amount
        let amount_out = adapter.get_amount_out(&pool, &swap.token_in, swap.amount_in)?;
        swap.amount_out = amount_out;
        swap.gas_used = adapter.estimate_gas(&pool);
        swap.gas_price = state.gas_price;

        // Update pool reserves
        let mut pool = pool.clone();
        let _ = adapter.execute_swap(
            &mut pool,
            swap.sender,
            swap.token_in,
            swap.amount_in,
            U256::ZERO, // No slippage check for market swaps
            swap.block_number,
        )?;

        state.set_pool(pool);

        Ok(swap)
    }

    /// Execute a bot action
    async fn execute_bot_action(&mut self, action: BotAction) {
        let bot_id = action.bot_id.clone();
        let action_start = Instant::now();

        if action.action_type == ActionType::Skip {
            return;
        }

        let bot_address = match self.bot_addresses.get(&bot_id) {
            Some(addr) => *addr,
            None => {
                warn!(bot_id = %bot_id, "Unknown bot tried to act");
                return;
            }
        };

        // Execute each swap in the action
        for intent in &action.swaps {
            let result = self.execute_swap_intent(&bot_id, bot_address, intent).await;

            match result {
                Ok(swap) => {
                    // Calculate profit (simplified - would need price oracle for accurate USD)
                    let profit_usd = dec!(0); // TODO: Calculate actual profit

                    let trade = TradeRecord::success(
                        &swap,
                        profit_usd,
                        action_start.elapsed().as_millis() as u64,
                    );
                    self.metrics.record_trade(&bot_id, trade);
                }
                Err(e) => {
                    warn!(bot_id = %bot_id, error = %e, "Bot action failed");

                    let trade = TradeRecord::failure(
                        &action,
                        e.to_string(),
                        U256::from(150_000u64), // Gas still consumed
                    );
                    self.metrics.record_trade(&bot_id, trade);
                }
            }
        }
    }

    /// Execute a single swap intent
    async fn execute_swap_intent(
        &self,
        bot_id: &str,
        bot_address: Address,
        intent: &SwapIntent,
    ) -> Result<SimSwap> {
        let state = self.state.write();

        // Check bot has sufficient balance
        let balance = state.get_balance(&bot_address, &intent.token_in);
        if balance < intent.amount_in {
            return Err(Error::InsufficientBalance {
                token: intent.token_in.to_string(),
                needed: intent.amount_in.to_string(),
                available: balance.to_string(),
            });
        }

        // Get pool and adapter
        let pool = state.get_pool(&intent.pool)
            .ok_or_else(|| Error::PoolNotFound(intent.pool.to_string()))?;

        let adapter = self.dex_registry.get_for_pool(&pool)
            .ok_or_else(|| Error::UnsupportedDex(pool.dex_type.to_string()))?;

        // Execute swap
        let mut pool = pool.clone();
        let swap = adapter.execute_swap(
            &mut pool,
            bot_address,
            intent.token_in,
            intent.amount_in,
            intent.min_amount_out,
            state.block_number,
        )?;

        // Update state
        state.set_pool(pool);

        // Update bot balances
        state.sub_balance(&bot_address, &intent.token_in, intent.amount_in)?;
        state.add_balance(&bot_address, &swap.token_out, swap.amount_out);

        Ok(swap.with_bot(bot_id))
    }

    /// Notify all bots of block start
    async fn notify_bots_block_start(&mut self, block: &SimBlock) {
        let state = self.state.read();
        for bot in &mut self.bots {
            bot.on_block_start(block, &state).await;
        }
    }

    /// Notify all bots of a swap and collect their actions
    async fn notify_bots_swap(&mut self, swap: &SimSwap) {
        let state = self.state.read();
        let mut actions = Vec::new();

        for bot in &mut self.bots {
            if let Some(action) = bot.on_swap(swap, &state).await {
                actions.push(action);
            }
        }

        drop(state); // Release lock before queuing events

        // Queue bot actions
        for action in actions {
            self.events.push(Event::BotAction(action));
        }
    }

    /// Notify all bots of block end
    async fn notify_bots_block_end(&mut self, block: &SimBlock) {
        let state = self.state.read();
        for bot in &mut self.bots {
            bot.on_block_end(block, &state).await;
        }
    }
}

/// Report from a simulation run
pub use crate::metrics::SimulationReport;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::PassiveBot;

    fn create_test_engine() -> SimulationEngine {
        let config = SimConfig::arbitrum().with_seed(12345);
        let engine = SimulationEngine::new(config);

        // Add a test pool
        let token0 = Token::new(Address::repeat_byte(0x01), "WETH", "Wrapped Ether", 18);
        let token1 = Token::new(Address::repeat_byte(0x02), "USDC", "USD Coin", 6);

        let pool = SimPool::new_v2(
            Address::repeat_byte(0x10),
            "uniswap-v2-test",
            token0,
            token1,
            U256::from(10_000_000_000_000_000_000u128),
            U256::from(20_000_000_000u64),
            30,
            DexType::UniswapV2,
            ChainId::Arbitrum,
        );

        engine.add_pool(pool);
        engine
    }

    #[test]
    fn test_engine_creation() {
        let engine = create_test_engine();
        assert_eq!(engine.bots.len(), 0);
        assert_eq!(engine.state.read().pool_count(), 1);
    }

    #[test]
    fn test_register_bot() {
        let mut engine = create_test_engine();

        let bot = PassiveBot::new("test_bot");
        engine.register_bot(Box::new(bot), dec!(10000)).unwrap();

        assert_eq!(engine.bots.len(), 1);
        assert!(engine.bot_addresses.contains_key("test_bot"));
    }

    #[test]
    fn test_duplicate_bot_registration() {
        let mut engine = create_test_engine();

        let bot1 = PassiveBot::new("test_bot");
        engine.register_bot(Box::new(bot1), dec!(10000)).unwrap();

        let bot2 = PassiveBot::new("test_bot");
        let result = engine.register_bot(Box::new(bot2), dec!(10000));

        assert!(matches!(result, Err(Error::BotRegistration(_))));
    }

    #[tokio::test]
    async fn test_run_simulation() {
        let mut engine = create_test_engine();

        let bot = PassiveBot::new("passive_bot");
        engine.register_bot(Box::new(bot), dec!(10000)).unwrap();

        let report = engine.run(10).await.unwrap();

        assert_eq!(report.blocks_processed, 10);
    }

    #[tokio::test]
    async fn test_deterministic_with_seed() {
        // Run twice with same seed
        let config = SimConfig::arbitrum().with_seed(42);

        let mut engine1 = SimulationEngine::with_scenario(
            config.clone(),
            ScenarioConfig { seed: 42, ..Default::default() },
        );
        engine1.add_pool(create_test_pool());
        let report1 = engine1.run(5).await.unwrap();

        let mut engine2 = SimulationEngine::with_scenario(
            config,
            ScenarioConfig { seed: 42, ..Default::default() },
        );
        engine2.add_pool(create_test_pool());
        let report2 = engine2.run(5).await.unwrap();

        // Should have same number of swaps
        assert_eq!(report1.total_swaps, report2.total_swaps);
    }

    fn create_test_pool() -> SimPool {
        let token0 = Token::new(Address::repeat_byte(0x01), "WETH", "Wrapped Ether", 18);
        let token1 = Token::new(Address::repeat_byte(0x02), "USDC", "USD Coin", 6);

        SimPool::new_v2(
            Address::repeat_byte(0x10),
            "uniswap-v2-test",
            token0,
            token1,
            U256::from(10_000_000_000_000_000_000u128),
            U256::from(20_000_000_000u64),
            30,
            DexType::UniswapV2,
            ChainId::Arbitrum,
        )
    }
}
