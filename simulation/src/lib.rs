//! # DeFi Simulation Framework
//!
//! A simulation framework for testing arbitrage bots and running competitive arenas
//! where bots compete against each other in a realistic DeFi environment.
//!
//! ## Features
//!
//! - **Event-driven simulation engine** with discrete time stepping for deterministic replays
//! - **Chain adapters** for forking real blockchain state (Arbitrum, Optimism, HyperEVM)
//! - **DEX adapters** for simulating swaps (Uniswap V2/V3, Camelot, Velodrome, etc.)
//! - **Bot interface** via Rust trait (fast) or gRPC (language-agnostic)
//! - **Scenario engine** for generating realistic swap patterns
//! - **Metrics collection** for bot performance analysis
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Simulation Engine                             │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
//! │  │ Event Queue  │  │ State Manager│  │ Time Control │          │
//! │  │ (Priority)   │  │ (Fork/Clone) │  │ (Discrete)   │          │
//! │  └──────────────┘  └──────────────┘  └──────────────┘          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                    DEX Simulators                                │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
//! │  │ Uniswap V2   │  │ Uniswap V3   │  │   Camelot    │          │
//! │  │  x * y = k   │  │ Concentrated │  │  V2-style    │          │
//! │  └──────────────┘  └──────────────┘  └──────────────┘          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                    Scenario Engine                               │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
//! │  │ Random Swaps │  │ Market Maker │  │ Flash Crash  │          │
//! │  │   Pattern    │  │   Pattern    │  │   Pattern    │          │
//! │  └──────────────┘  └──────────────┘  └──────────────┘          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                    Bot Interface                                 │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
//! │  │   Bot A      │  │   Bot B      │  │   Bot C      │          │
//! │  │  (Sneak)     │  │ (Competitor) │  │ (Competitor) │          │
//! │  └──────────────┘  └──────────────┘  └──────────────┘          │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```text
//! use sneak_simulation::{SimulationEngine, SimConfig, bot::ArbitrageBot};
//!
//! // Create simulation config for Arbitrum
//! let config = SimConfig::arbitrum();
//!
//! // Create simulation engine
//! let mut engine = SimulationEngine::new(config).await?;
//!
//! // Register your bot
//! engine.register_bot(Box::new(MyBot::new()));
//!
//! // Run simulation
//! let results = engine.run(1000).await?; // Run 1000 blocks
//!
//! // Analyze results
//! for (bot_id, metrics) in results.bot_metrics {
//!     println!("{}: profit = {}", bot_id, metrics.total_profit);
//! }
//! ```

pub mod types;
pub mod error;
pub mod engine;
pub mod state;
pub mod event;
pub mod chain;
pub mod dex;
pub mod bot;
pub mod scenario;
pub mod metrics;

// Re-exports
pub use types::*;
pub use error::{Error, Result};
pub use engine::SimulationEngine;
pub use state::SimState;
pub use event::{Event, EventQueue};
pub use bot::ArbitrageBot;
pub use scenario::ScenarioGenerator;
pub use metrics::MetricsCollector;
pub use chain::{AnvilConfig, AnvilManager, RpcChainAdapter, ChainAdapter, check_anvil_installed};
