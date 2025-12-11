//! Simulation state management.
//!
//! The `SimState` holds all mutable state for the simulation:
//! - Pool reserves and liquidity
//! - Account balances
//! - Current block and gas prices

use crate::{ChainId, DexType, SimPool, Token, Result, Error};
use alloy_primitives::{Address, U256};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Simulation state container
#[derive(Debug)]
pub struct SimState {
    /// Chain ID
    pub chain_id: ChainId,
    /// Current block number
    pub block_number: u64,
    /// Current timestamp
    pub timestamp: DateTime<Utc>,
    /// Current gas price
    pub gas_price: U256,
    /// Current base fee
    pub base_fee: U256,
    /// Pools indexed by address
    pools: DashMap<Address, SimPool>,
    /// Balances: address -> token -> balance
    balances: DashMap<Address, DashMap<Address, U256>>,
    /// Known tokens
    tokens: DashMap<Address, Token>,
}

impl SimState {
    /// Create a new empty state
    pub fn new(chain_id: ChainId) -> Self {
        Self {
            chain_id,
            block_number: 0,
            timestamp: Utc::now(),
            gas_price: U256::from(100_000_000u64), // 0.1 gwei
            base_fee: U256::from(100_000_000u64),
            pools: DashMap::new(),
            balances: DashMap::new(),
            tokens: DashMap::new(),
        }
    }

    /// Add or update a pool
    pub fn set_pool(&self, pool: SimPool) {
        // Also register the tokens
        self.tokens.insert(pool.token0.address, pool.token0.clone());
        self.tokens.insert(pool.token1.address, pool.token1.clone());
        self.pools.insert(pool.address, pool);
    }

    /// Get a pool by address
    pub fn get_pool(&self, address: &Address) -> Option<SimPool> {
        self.pools.get(address).map(|p| p.clone())
    }

    /// Get all pools
    pub fn get_pools(&self) -> Vec<SimPool> {
        self.pools.iter().map(|p| p.value().clone()).collect()
    }

    /// Get pools containing a specific token
    pub fn get_pools_for_token(&self, token: &Address) -> Vec<SimPool> {
        self.pools
            .iter()
            .filter(|p| p.token0.address == *token || p.token1.address == *token)
            .map(|p| p.value().clone())
            .collect()
    }

    /// Get pools of a specific DEX type
    pub fn get_pools_by_dex(&self, dex_type: DexType) -> Vec<SimPool> {
        self.pools
            .iter()
            .filter(|p| p.dex_type == dex_type)
            .map(|p| p.value().clone())
            .collect()
    }

    /// Update pool reserves
    pub fn update_reserves(
        &self,
        pool_address: &Address,
        reserve0: U256,
        reserve1: U256,
    ) -> Result<()> {
        if let Some(mut pool) = self.pools.get_mut(pool_address) {
            pool.reserve0 = reserve0;
            pool.reserve1 = reserve1;
            pool.last_updated = Utc::now();
            Ok(())
        } else {
            Err(Error::PoolNotFound(pool_address.to_string()))
        }
    }

    /// Get balance of a token for an address
    pub fn get_balance(&self, address: &Address, token: &Address) -> U256 {
        self.balances
            .get(address)
            .and_then(|tokens| tokens.get(token).map(|b| *b))
            .unwrap_or(U256::ZERO)
    }

    /// Set balance of a token for an address
    pub fn set_balance(&self, address: &Address, token: &Address, amount: U256) {
        self.balances
            .entry(*address)
            .or_insert_with(DashMap::new)
            .insert(*token, amount);
    }

    /// Add to balance (returns new balance)
    pub fn add_balance(&self, address: &Address, token: &Address, amount: U256) -> U256 {
        let entry = self.balances.entry(*address).or_insert_with(DashMap::new);
        let mut balance = entry.entry(*token).or_insert(U256::ZERO);
        *balance += amount;
        *balance
    }

    /// Subtract from balance (returns Result with new balance or InsufficientBalance error)
    pub fn sub_balance(&self, address: &Address, token: &Address, amount: U256) -> Result<U256> {
        let entry = self.balances.entry(*address).or_insert_with(DashMap::new);
        let mut balance = entry.entry(*token).or_insert(U256::ZERO);

        if *balance < amount {
            return Err(Error::InsufficientBalance {
                token: token.to_string(),
                needed: amount.to_string(),
                available: balance.to_string(),
            });
        }

        *balance -= amount;
        Ok(*balance)
    }

    /// Transfer tokens between addresses
    pub fn transfer(
        &self,
        from: &Address,
        to: &Address,
        token: &Address,
        amount: U256,
    ) -> Result<()> {
        self.sub_balance(from, token, amount)?;
        self.add_balance(to, token, amount);
        Ok(())
    }

    /// Get a token by address
    pub fn get_token(&self, address: &Address) -> Option<Token> {
        self.tokens.get(address).map(|t| t.clone())
    }

    /// Register a token
    pub fn register_token(&self, token: Token) {
        self.tokens.insert(token.address, token);
    }

    /// Get all balances for an address
    pub fn get_all_balances(&self, address: &Address) -> HashMap<Address, U256> {
        self.balances
            .get(address)
            .map(|tokens| {
                tokens
                    .iter()
                    .map(|e| (*e.key(), *e.value()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Advance to next block
    pub fn advance_block(&mut self) {
        self.block_number += 1;
        // Timestamp advances by block time (will be set by engine)
    }

    /// Create a snapshot of the current state
    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            chain_id: self.chain_id,
            block_number: self.block_number,
            timestamp: self.timestamp,
            gas_price: self.gas_price,
            base_fee: self.base_fee,
            pools: self.pools.iter().map(|p| (p.key().clone(), p.value().clone())).collect(),
            balances: self.balances
                .iter()
                .map(|e| {
                    let address = *e.key();
                    let tokens: HashMap<Address, U256> = e.value()
                        .iter()
                        .map(|t| (*t.key(), *t.value()))
                        .collect();
                    (address, tokens)
                })
                .collect(),
        }
    }

    /// Restore from a snapshot
    pub fn restore(&mut self, snapshot: StateSnapshot) {
        self.chain_id = snapshot.chain_id;
        self.block_number = snapshot.block_number;
        self.timestamp = snapshot.timestamp;
        self.gas_price = snapshot.gas_price;
        self.base_fee = snapshot.base_fee;

        self.pools.clear();
        for (addr, pool) in snapshot.pools {
            self.pools.insert(addr, pool);
        }

        self.balances.clear();
        for (addr, tokens) in snapshot.balances {
            let token_map = DashMap::new();
            for (token, balance) in tokens {
                token_map.insert(token, balance);
            }
            self.balances.insert(addr, token_map);
        }
    }

    /// Get number of pools
    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }

    /// Get number of unique addresses with balances
    pub fn address_count(&self) -> usize {
        self.balances.len()
    }
}

/// Serializable state snapshot for save/restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub chain_id: ChainId,
    pub block_number: u64,
    pub timestamp: DateTime<Utc>,
    pub gas_price: U256,
    pub base_fee: U256,
    pub pools: HashMap<Address, SimPool>,
    pub balances: HashMap<Address, HashMap<Address, U256>>,
}

impl StateSnapshot {
    /// Save snapshot to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Error::from)
    }

    /// Load snapshot from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_token(symbol: &str) -> Token {
        Token::new(
            Address::repeat_byte(symbol.as_bytes()[0]),
            symbol,
            format!("{} Token", symbol),
            18,
        )
    }

    fn create_test_pool() -> SimPool {
        let token0 = create_test_token("WETH");
        let token1 = create_test_token("USDC");
        SimPool::new_v2(
            Address::repeat_byte(0x01),
            "uniswap-v2-test",
            token0,
            token1,
            U256::from(1000u64),
            U256::from(2000u64),
            30,
            DexType::UniswapV2,
            ChainId::Arbitrum,
        )
    }

    #[test]
    fn test_state_pool_operations() {
        let state = SimState::new(ChainId::Arbitrum);
        let pool = create_test_pool();
        let pool_addr = pool.address;

        state.set_pool(pool.clone());

        let retrieved = state.get_pool(&pool_addr).unwrap();
        assert_eq!(retrieved.address, pool_addr);
        assert_eq!(retrieved.reserve0, U256::from(1000u64));
    }

    #[test]
    fn test_state_balance_operations() {
        let state = SimState::new(ChainId::Arbitrum);
        let user = Address::repeat_byte(0xAA);
        let token = Address::repeat_byte(0xBB);

        // Initial balance is zero
        assert_eq!(state.get_balance(&user, &token), U256::ZERO);

        // Set balance
        state.set_balance(&user, &token, U256::from(1000u64));
        assert_eq!(state.get_balance(&user, &token), U256::from(1000u64));

        // Add to balance
        let new_balance = state.add_balance(&user, &token, U256::from(500u64));
        assert_eq!(new_balance, U256::from(1500u64));

        // Subtract from balance
        let new_balance = state.sub_balance(&user, &token, U256::from(300u64)).unwrap();
        assert_eq!(new_balance, U256::from(1200u64));
    }

    #[test]
    fn test_state_insufficient_balance() {
        let state = SimState::new(ChainId::Arbitrum);
        let user = Address::repeat_byte(0xAA);
        let token = Address::repeat_byte(0xBB);

        state.set_balance(&user, &token, U256::from(100u64));

        let result = state.sub_balance(&user, &token, U256::from(200u64));
        assert!(matches!(result, Err(Error::InsufficientBalance { .. })));
    }

    #[test]
    fn test_state_transfer() {
        let state = SimState::new(ChainId::Arbitrum);
        let alice = Address::repeat_byte(0xAA);
        let bob = Address::repeat_byte(0xBB);
        let token = Address::repeat_byte(0xCC);

        state.set_balance(&alice, &token, U256::from(1000u64));

        state.transfer(&alice, &bob, &token, U256::from(300u64)).unwrap();

        assert_eq!(state.get_balance(&alice, &token), U256::from(700u64));
        assert_eq!(state.get_balance(&bob, &token), U256::from(300u64));
    }

    #[test]
    fn test_state_snapshot_restore() {
        let mut state = SimState::new(ChainId::Arbitrum);
        let pool = create_test_pool();
        state.set_pool(pool);
        state.block_number = 100;

        let user = Address::repeat_byte(0xAA);
        let token = Address::repeat_byte(0xBB);
        state.set_balance(&user, &token, U256::from(5000u64));

        // Take snapshot
        let snapshot = state.snapshot();

        // Modify state
        state.block_number = 200;
        state.set_balance(&user, &token, U256::from(1000u64));

        // Restore
        state.restore(snapshot);

        assert_eq!(state.block_number, 100);
        assert_eq!(state.get_balance(&user, &token), U256::from(5000u64));
    }

    #[test]
    fn test_snapshot_serialization() {
        let state = SimState::new(ChainId::Arbitrum);
        state.set_pool(create_test_pool());

        let snapshot = state.snapshot();
        let json = snapshot.to_json().unwrap();
        let restored = StateSnapshot::from_json(&json).unwrap();

        assert_eq!(restored.chain_id, ChainId::Arbitrum);
        assert_eq!(restored.pools.len(), 1);
    }
}
