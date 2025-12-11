//! Event system for the simulation engine.
//!
//! Events are processed in priority order within each block.
//! Higher priority events are processed first.

use crate::{BotAction, SimSwap};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Simulation event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    /// New block started
    BlockStart {
        block_number: u64,
        timestamp: DateTime<Utc>,
    },
    /// Block ended
    BlockEnd {
        block_number: u64,
    },
    /// Scenario-generated swap (market activity)
    MarketSwap(SimSwap),
    /// Bot action to be executed
    BotAction(BotAction),
    /// Simulation ended
    SimulationEnd,
}

impl Event {
    /// Get the priority of this event (higher = processed first)
    pub fn priority(&self) -> i32 {
        match self {
            Event::BlockStart { .. } => 1000, // Always first
            Event::MarketSwap(_) => 100,      // Market activity
            Event::BotAction(action) => 50 + action.priority, // Bots react to market
            Event::BlockEnd { .. } => -100,   // Always last in block
            Event::SimulationEnd => -1000,    // Very last
        }
    }

    /// Get the block number this event belongs to (if applicable)
    pub fn block_number(&self) -> Option<u64> {
        match self {
            Event::BlockStart { block_number, .. } => Some(*block_number),
            Event::BlockEnd { block_number } => Some(*block_number),
            Event::MarketSwap(swap) => Some(swap.block_number),
            Event::BotAction(_) => None, // Bot actions don't have inherent block
            Event::SimulationEnd => None,
        }
    }
}

/// Wrapper for priority queue ordering
#[derive(Debug)]
struct PrioritizedEvent {
    event: Event,
    sequence: u64, // For stable ordering within same priority
}

impl PartialEq for PrioritizedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.event.priority() == other.event.priority() && self.sequence == other.sequence
    }
}

impl Eq for PrioritizedEvent {}

impl PartialOrd for PrioritizedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first, then lower sequence (FIFO within same priority)
        match self.event.priority().cmp(&other.event.priority()) {
            Ordering::Equal => other.sequence.cmp(&self.sequence), // Reverse for FIFO
            other => other,
        }
    }
}

/// Priority queue for simulation events
#[derive(Debug)]
pub struct EventQueue {
    heap: BinaryHeap<PrioritizedEvent>,
    sequence: u64,
}

impl EventQueue {
    /// Create a new empty event queue
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            sequence: 0,
        }
    }

    /// Push an event onto the queue
    pub fn push(&mut self, event: Event) {
        self.sequence += 1;
        self.heap.push(PrioritizedEvent {
            event,
            sequence: self.sequence,
        });
    }

    /// Pop the highest priority event
    pub fn pop(&mut self) -> Option<Event> {
        self.heap.pop().map(|pe| pe.event)
    }

    /// Peek at the highest priority event without removing it
    pub fn peek(&self) -> Option<&Event> {
        self.heap.peek().map(|pe| &pe.event)
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Get the number of events in the queue
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Clear all events
    pub fn clear(&mut self) {
        self.heap.clear();
        self.sequence = 0;
    }

    /// Push multiple events
    pub fn push_all(&mut self, events: impl IntoIterator<Item = Event>) {
        for event in events {
            self.push(event);
        }
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};

    #[test]
    fn test_event_priority_ordering() {
        let mut queue = EventQueue::new();

        // Add events in reverse priority order
        queue.push(Event::SimulationEnd);
        queue.push(Event::BlockEnd { block_number: 1 });
        queue.push(Event::BotAction(crate::BotAction::skip("bot1")));
        queue.push(Event::MarketSwap(SimSwap::new(
            Address::ZERO,
            Address::ZERO,
            Address::ZERO,
            Address::ZERO,
            U256::ZERO,
            U256::ZERO,
            1,
        )));
        queue.push(Event::BlockStart {
            block_number: 1,
            timestamp: Utc::now(),
        });

        // Should come out in priority order
        assert!(matches!(queue.pop(), Some(Event::BlockStart { .. })));
        assert!(matches!(queue.pop(), Some(Event::MarketSwap(_))));
        assert!(matches!(queue.pop(), Some(Event::BotAction(_))));
        assert!(matches!(queue.pop(), Some(Event::BlockEnd { .. })));
        assert!(matches!(queue.pop(), Some(Event::SimulationEnd)));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_event_fifo_within_priority() {
        let mut queue = EventQueue::new();

        // Add multiple bot actions with same priority
        queue.push(Event::BotAction(crate::BotAction::skip("bot1")));
        queue.push(Event::BotAction(crate::BotAction::skip("bot2")));
        queue.push(Event::BotAction(crate::BotAction::skip("bot3")));

        // Should come out in FIFO order
        if let Some(Event::BotAction(action)) = queue.pop() {
            assert_eq!(action.bot_id, "bot1");
        }
        if let Some(Event::BotAction(action)) = queue.pop() {
            assert_eq!(action.bot_id, "bot2");
        }
        if let Some(Event::BotAction(action)) = queue.pop() {
            assert_eq!(action.bot_id, "bot3");
        }
    }

    #[test]
    fn test_bot_action_priority() {
        let mut queue = EventQueue::new();

        // Bot with higher priority should be processed first
        queue.push(Event::BotAction(crate::BotAction::skip("low_priority")));
        queue.push(Event::BotAction(
            crate::BotAction::skip("high_priority").with_priority(100),
        ));

        if let Some(Event::BotAction(action)) = queue.pop() {
            assert_eq!(action.bot_id, "high_priority");
        }
        if let Some(Event::BotAction(action)) = queue.pop() {
            assert_eq!(action.bot_id, "low_priority");
        }
    }
}
