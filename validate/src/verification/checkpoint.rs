//! Checkpoint-based timer verification
//!
//! Allows defining expected timer states at specific combat times
//! and verifying that actual timer behavior matches expectations.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Expected timer state at a checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedTimer {
    /// Timer definition ID
    pub id: String,

    /// Expected remaining time range [min, max] in seconds
    /// If None, timer should be active but remaining time is not checked
    #[serde(default)]
    pub remaining_secs: Option<(f32, f32)>,
}

/// A checkpoint defines expected timer state at a specific combat time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Combat time in seconds when to check
    pub at_secs: f32,

    /// Timers that should be active at this time
    #[serde(default)]
    pub active_timers: Vec<ExpectedTimer>,

    /// Timers that should have fired by this time
    #[serde(default)]
    pub timers_fired: Vec<String>,

    /// Alerts that should have fired by this time
    #[serde(default)]
    pub alerts_fired: Vec<String>,

    /// Optional description for debugging
    #[serde(default)]
    pub description: Option<String>,
}

/// Full expectations file for a boss encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expectations {
    /// Metadata
    pub meta: ExpectationsMeta,

    /// Checkpoints to verify
    #[serde(rename = "checkpoint")]
    pub checkpoints: Vec<Checkpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectationsMeta {
    /// Boss definition ID this expectation file is for
    pub boss_id: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Tolerance in seconds for timing comparisons (default 0.5)
    #[serde(default = "default_tolerance")]
    pub tolerance_secs: f32,
}

fn default_tolerance() -> f32 {
    0.5
}

impl Expectations {
    /// Load expectations from a TOML file
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let expectations: Expectations = toml::from_str(&content)?;
        Ok(expectations)
    }
}

/// Result of verifying a single checkpoint
#[derive(Debug, Clone)]
pub struct CheckpointResult {
    pub checkpoint_idx: usize,
    pub at_secs: f32,
    pub passed: bool,
    pub failures: Vec<String>,
}

/// Overall verification result
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub checkpoints_passed: u32,
    pub checkpoints_total: u32,
    pub results: Vec<CheckpointResult>,
}

impl VerificationResult {
    pub fn passed(&self) -> bool {
        self.checkpoints_passed == self.checkpoints_total
    }
}

/// Verifies timer behavior against expected checkpoints
#[derive(Debug)]
pub struct CheckpointVerifier {
    expectations: Expectations,
    current_checkpoint_idx: usize,
    results: Vec<CheckpointResult>,

    /// Timers that have been started (timer_id -> start_time_secs)
    timers_started: Vec<(String, f32)>,

    /// Alerts that have fired (alert_id)
    alerts_fired: Vec<String>,
}

impl CheckpointVerifier {
    pub fn new(expectations: Expectations) -> Self {
        Self {
            expectations,
            current_checkpoint_idx: 0,
            results: Vec::new(),
            timers_started: Vec::new(),
            alerts_fired: Vec::new(),
        }
    }

    /// Record that a timer started
    pub fn record_timer_start(&mut self, timer_id: &str, combat_time_secs: f32) {
        self.timers_started
            .push((timer_id.to_string(), combat_time_secs));
    }

    /// Record that an alert fired
    pub fn record_alert(&mut self, alert_id: &str) {
        self.alerts_fired.push(alert_id.to_string());
    }

    /// Check if there's a checkpoint at or before the given combat time
    /// Returns checkpoints that should be verified
    pub fn check_time(
        &mut self,
        combat_time_secs: f32,
        active_timers: &[(String, f32)],
    ) -> Option<CheckpointResult> {
        if self.current_checkpoint_idx >= self.expectations.checkpoints.len() {
            return None;
        }

        let checkpoint = &self.expectations.checkpoints[self.current_checkpoint_idx];
        let tolerance = self.expectations.meta.tolerance_secs;

        // Check if we've reached this checkpoint
        if combat_time_secs < checkpoint.at_secs - tolerance {
            return None;
        }

        // We've reached the checkpoint - verify it
        let mut failures = Vec::new();

        // Check active timers
        for expected in &checkpoint.active_timers {
            let found = active_timers.iter().find(|(id, _)| id == &expected.id);

            match found {
                None => {
                    failures.push(format!(
                        "Timer '{}' should be active but is not",
                        expected.id
                    ));
                }
                Some((_, remaining)) => {
                    if let Some((min, max)) = expected.remaining_secs {
                        if *remaining < min - tolerance || *remaining > max + tolerance {
                            failures.push(format!(
                                "Timer '{}' remaining {:.1}s not in expected range [{:.1}, {:.1}]",
                                expected.id, remaining, min, max
                            ));
                        }
                    }
                }
            }
        }

        // Check timers that should have fired
        for timer_id in &checkpoint.timers_fired {
            if !self.timers_started.iter().any(|(id, _)| id == timer_id) {
                failures.push(format!("Timer '{}' should have fired but hasn't", timer_id));
            }
        }

        // Check alerts that should have fired
        for alert_id in &checkpoint.alerts_fired {
            if !self.alerts_fired.contains(alert_id) {
                failures.push(format!("Alert '{}' should have fired but hasn't", alert_id));
            }
        }

        let result = CheckpointResult {
            checkpoint_idx: self.current_checkpoint_idx,
            at_secs: checkpoint.at_secs,
            passed: failures.is_empty(),
            failures,
        };

        self.results.push(result.clone());
        self.current_checkpoint_idx += 1;

        Some(result)
    }

    /// Get final verification result
    pub fn finalize(self) -> VerificationResult {
        let passed = self.results.iter().filter(|r| r.passed).count() as u32;
        let total = self.results.len() as u32;

        VerificationResult {
            checkpoints_passed: passed,
            checkpoints_total: total,
            results: self.results,
        }
    }

    /// Check if all checkpoints have been processed
    pub fn is_complete(&self) -> bool {
        self.current_checkpoint_idx >= self.expectations.checkpoints.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_expectations() -> Expectations {
        Expectations {
            meta: ExpectationsMeta {
                boss_id: "test_boss".to_string(),
                description: None,
                tolerance_secs: 0.5,
            },
            checkpoints: vec![Checkpoint {
                at_secs: 15.0,
                active_timers: vec![ExpectedTimer {
                    id: "test_timer".to_string(),
                    remaining_secs: Some((10.0, 12.0)),
                }],
                timers_fired: vec!["test_timer".to_string()],
                alerts_fired: vec![],
                description: Some("First checkpoint".to_string()),
            }],
        }
    }

    #[test]
    fn test_checkpoint_pass() {
        let exp = sample_expectations();
        let mut verifier = CheckpointVerifier::new(exp);

        verifier.record_timer_start("test_timer", 5.0);

        // Active timers: test_timer with 11s remaining (started at 5s, now at 15s, duration unknown)
        let active = vec![("test_timer".to_string(), 11.0)];
        let result = verifier.check_time(15.0, &active);

        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.passed, "Failures: {:?}", r.failures);
    }

    #[test]
    fn test_checkpoint_fail_missing_timer() {
        let exp = sample_expectations();
        let mut verifier = CheckpointVerifier::new(exp);

        verifier.record_timer_start("test_timer", 5.0);

        // No active timers
        let active: Vec<(String, f32)> = vec![];
        let result = verifier.check_time(15.0, &active);

        assert!(result.is_some());
        let r = result.unwrap();
        assert!(!r.passed);
        assert!(r.failures.iter().any(|f| f.contains("should be active")));
    }

    #[test]
    fn test_checkpoint_fail_wrong_remaining() {
        let exp = sample_expectations();
        let mut verifier = CheckpointVerifier::new(exp);

        verifier.record_timer_start("test_timer", 5.0);

        // Timer active but with wrong remaining time
        let active = vec![("test_timer".to_string(), 5.0)]; // 5s remaining, expected 10-12s
        let result = verifier.check_time(15.0, &active);

        assert!(result.is_some());
        let r = result.unwrap();
        assert!(!r.passed);
        assert!(
            r.failures
                .iter()
                .any(|f| f.contains("not in expected range"))
        );
    }
}
