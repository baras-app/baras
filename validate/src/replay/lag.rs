//! Lag simulation for realistic timer testing
//!
//! In live mode, there's always some delay between when an event occurs in-game
//! and when BARAS processes it (file I/O, polling intervals, etc.). This module
//! simulates that lag to catch timing-sensitive bugs.

use std::time::Duration;

/// Simulates realistic file I/O lag patterns
#[derive(Debug, Clone)]
pub struct LagSimulator {
    /// Base lag in milliseconds (typical file read latency)
    base_lag_ms: u64,

    /// Random jitter range in milliseconds
    jitter_ms: u64,

    /// Probability of a lag spike (filesystem hiccups)
    spike_probability: f32,

    /// Duration of lag spikes in milliseconds
    spike_lag_ms: u64,

    /// Whether lag simulation is enabled
    enabled: bool,

    /// Simple PRNG state for deterministic testing
    rng_state: u64,
}

impl Default for LagSimulator {
    fn default() -> Self {
        Self {
            base_lag_ms: 30,         // 30ms typical file I/O
            jitter_ms: 20,           // ±20ms variation
            spike_probability: 0.01, // 1% chance of spike
            spike_lag_ms: 200,       // 200ms spike duration
            enabled: true,
            rng_state: 12345,
        }
    }
}

impl LagSimulator {
    /// Create a new lag simulator with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a disabled lag simulator (for instant mode)
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Create a lag simulator with custom parameters
    pub fn custom(
        base_lag_ms: u64,
        jitter_ms: u64,
        spike_probability: f32,
        spike_lag_ms: u64,
    ) -> Self {
        Self {
            base_lag_ms,
            jitter_ms,
            spike_probability,
            spike_lag_ms,
            enabled: true,
            rng_state: 12345,
        }
    }

    /// Seed the RNG for deterministic behavior in tests
    pub fn seed(&mut self, seed: u64) {
        self.rng_state = seed;
    }

    /// Get the next simulated lag duration
    pub fn next_lag(&mut self) -> Duration {
        if !self.enabled {
            return Duration::ZERO;
        }

        let mut lag = self.base_lag_ms;

        // Add jitter using simple LCG PRNG
        if self.jitter_ms > 0 {
            let jitter = self.next_random() % (self.jitter_ms + 1);
            lag += jitter;
        }

        // Occasional spikes
        let spike_roll = (self.next_random() % 10000) as f32 / 10000.0;
        if spike_roll < self.spike_probability {
            lag += self.spike_lag_ms;
        }

        Duration::from_millis(lag)
    }

    /// Get lag in milliseconds (convenience method)
    pub fn next_lag_ms(&mut self) -> u64 {
        self.next_lag().as_millis() as u64
    }

    /// Simple LCG PRNG for deterministic jitter
    fn next_random(&mut self) -> u64 {
        // LCG parameters from Numerical Recipes
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        self.rng_state >> 33
    }

    /// Check if lag simulation is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable lag simulation
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_lag() {
        let mut lag = LagSimulator::disabled();
        assert_eq!(lag.next_lag(), Duration::ZERO);
        assert_eq!(lag.next_lag(), Duration::ZERO);
    }

    #[test]
    fn test_deterministic_with_seed() {
        let mut lag1 = LagSimulator::new();
        lag1.seed(42);

        let mut lag2 = LagSimulator::new();
        lag2.seed(42);

        // Same seed should produce same sequence
        for _ in 0..10 {
            assert_eq!(lag1.next_lag(), lag2.next_lag());
        }
    }

    #[test]
    fn test_lag_in_expected_range() {
        let mut lag = LagSimulator::custom(30, 20, 0.0, 0); // No spikes
        lag.seed(12345);

        for _ in 0..100 {
            let lag_ms = lag.next_lag().as_millis() as u64;
            assert!(lag_ms >= 30, "Lag {} should be >= base 30ms", lag_ms);
            assert!(lag_ms <= 50, "Lag {} should be <= base+jitter 50ms", lag_ms);
        }
    }

    #[test]
    fn test_custom_parameters() {
        let mut lag = LagSimulator::custom(100, 50, 0.0, 0);
        lag.seed(99);

        for _ in 0..50 {
            let lag_ms = lag.next_lag().as_millis() as u64;
            assert!(lag_ms >= 100);
            assert!(lag_ms <= 150);
        }
    }
}
