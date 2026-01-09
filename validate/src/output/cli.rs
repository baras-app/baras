//! Colored CLI output for timer events
//!
//! Formats timer starts, stops, alerts, and phase changes with
//! colored output for easy visual parsing.

use std::collections::HashMap;
use std::io::{self, Write};

use baras_core::encounter::ChallengeValue;
use chrono::NaiveDateTime;

/// Output verbosity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputLevel {
    /// Only show summary at end
    Quiet,
    /// Show timer events and alerts (default)
    Normal,
    /// Show all events including non-timer signals
    Verbose,
}

impl Default for OutputLevel {
    fn default() -> Self {
        Self::Normal
    }
}

/// A recorded phase span with start/end times
#[derive(Debug, Clone)]
pub struct PhaseSpan {
    pub phase_id: String,
    pub start_time: NaiveDateTime,
    pub end_time: Option<NaiveDateTime>,
}

/// Boss HP state for the current encounter
#[derive(Debug, Clone)]
pub struct BossHpState {
    pub name: String,
    pub npc_id: i64,
    pub current_hp: i64,
    pub max_hp: i64,
}

/// CLI output formatter with color support
#[derive(Debug)]
pub struct CliOutput {
    level: OutputLevel,
    combat_start: Option<NaiveDateTime>,
    use_colors: bool,
    timers_started: u32,
    timers_expired: u32,
    alerts_fired: u32,
    phase_changes: u32,
    counter_changes: u32,
    // Boss detection tracking
    boss_detected_in_combat: bool,
    pending_combat_start: Option<NaiveDateTime>,
    // Phase tracking
    phase_spans: Vec<PhaseSpan>,
    current_phase: Option<(String, NaiveDateTime)>,
    // Boss HP tracking for current encounter
    boss_hp: HashMap<i64, BossHpState>,
}

impl Default for CliOutput {
    fn default() -> Self {
        Self::new(OutputLevel::Normal)
    }
}

impl CliOutput {
    pub fn new(level: OutputLevel) -> Self {
        Self {
            level,
            combat_start: None,
            use_colors: atty::is(atty::Stream::Stdout),
            timers_started: 0,
            timers_expired: 0,
            alerts_fired: 0,
            phase_changes: 0,
            counter_changes: 0,
            boss_detected_in_combat: false,
            pending_combat_start: None,
            phase_spans: Vec::new(),
            current_phase: None,
            boss_hp: HashMap::new(),
        }
    }

    /// Check if we should output (boss detected in current combat)
    fn should_output(&self) -> bool {
        self.boss_detected_in_combat
    }

    /// Set combat start time for relative timestamps
    pub fn set_combat_start(&mut self, time: NaiveDateTime) {
        self.combat_start = Some(time);
    }

    /// Format timestamp relative to combat start
    pub fn format_time(&self, time: NaiveDateTime) -> String {
        if let Some(start) = self.combat_start {
            let delta = time - start;
            let secs = delta.num_milliseconds() as f32 / 1000.0;
            let mins = (secs / 60.0).floor() as u32;
            let secs_remainder = secs % 60.0;
            format!("{:02}:{:05.2}", mins, secs_remainder)
        } else {
            time.format("%H:%M:%S%.3f").to_string()
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ANSI Color Codes
    // ═══════════════════════════════════════════════════════════════════════════

    fn green(&self, text: &str) -> String {
        if self.use_colors {
            format!("\x1b[32m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    fn bright_green(&self, text: &str) -> String {
        if self.use_colors {
            format!("\x1b[92m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    fn yellow(&self, text: &str) -> String {
        if self.use_colors {
            format!("\x1b[33m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    fn red(&self, text: &str) -> String {
        if self.use_colors {
            format!("\x1b[31m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    fn cyan(&self, text: &str) -> String {
        if self.use_colors {
            format!("\x1b[36m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    fn magenta(&self, text: &str) -> String {
        if self.use_colors {
            format!("\x1b[35m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    fn dim(&self, text: &str) -> String {
        if self.use_colors {
            format!("\x1b[2m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    fn bold(&self, text: &str) -> String {
        if self.use_colors {
            format!("\x1b[1m{}\x1b[0m", text)
        } else {
            text.to_string()
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Event Output
    // ═══════════════════════════════════════════════════════════════════════════

    /// Log timer start
    pub fn timer_start(
        &mut self,
        time: NaiveDateTime,
        name: &str,
        duration_secs: f32,
        timer_id: &str,
    ) {
        self.timers_started += 1;
        if self.level < OutputLevel::Normal || !self.should_output() {
            return;
        }

        let time_str = self.format_time(time);
        let arrow = self.green("-->");
        let label = self.green("TIMER START:");
        let id = self.dim(&format!("[{}]", timer_id));

        println!(
            "[{}] {} {} \"{}\" ({:.1}s) {}",
            time_str, arrow, label, name, duration_secs, id
        );
    }

    /// Log timer expiration
    pub fn timer_expire(&mut self, time: NaiveDateTime, name: &str, timer_id: &str) {
        self.timers_expired += 1;
        if self.level < OutputLevel::Normal || !self.should_output() {
            return;
        }

        let time_str = self.format_time(time);
        let arrow = self.yellow("<--");
        let label = self.yellow("TIMER EXPIRE:");
        let id = self.dim(&format!("[{}]", timer_id));

        println!("[{}] {} {} \"{}\" {}", time_str, arrow, label, name, id);
    }

    /// Log timer cancellation
    pub fn timer_cancel(&mut self, time: NaiveDateTime, name: &str, timer_id: &str) {
        if self.level < OutputLevel::Normal || !self.should_output() {
            return;
        }

        let time_str = self.format_time(time);
        let arrow = self.dim("x--");
        let label = self.dim("TIMER CANCEL:");
        let id = self.dim(&format!("[{}]", timer_id));

        println!("[{}] {} {} \"{}\" {}", time_str, arrow, label, name, id);
    }

    /// Log entity death (for kill targets)
    pub fn entity_death(
        &mut self,
        time: NaiveDateTime,
        name: &str,
        npc_id: i64,
        is_kill_target: bool,
    ) {
        if self.level < OutputLevel::Normal || !self.should_output() {
            return;
        }

        let time_str = self.format_time(time);
        if is_kill_target {
            let marker = self.red("XXX");
            let label = self.red("KILL TARGET DEAD:");
            println!(
                "[{}] {} {} \"{}\" [{}]",
                time_str, marker, label, name, npc_id
            );
        } else if self.level >= OutputLevel::Verbose {
            let marker = self.dim("xxx");
            let label = self.dim("DEATH:");
            println!(
                "[{}] {} {} \"{}\" [{}]",
                time_str, marker, label, name, npc_id
            );
        }
    }

    /// Log alert fired
    pub fn alert(&mut self, time: NaiveDateTime, name: &str, text: &str) {
        self.alerts_fired += 1;
        if self.level < OutputLevel::Normal || !self.should_output() {
            return;
        }

        let time_str = self.format_time(time);
        let marker = self.red("!!!");
        let label = self.red("ALERT:");

        if text.is_empty() || text == name {
            println!("[{}] {} {} \"{}\"", time_str, marker, label, name);
        } else {
            println!(
                "[{}] {} {} \"{}\" - {}",
                time_str, marker, label, name, text
            );
        }
    }

    /// Log phase change - now collects phase spans instead of printing inline
    pub fn phase_change(&mut self, time: NaiveDateTime, _old_phase: Option<&str>, new_phase: &str) {
        self.phase_changes += 1;

        // End the previous phase
        if let Some((phase_id, start_time)) = self.current_phase.take() {
            self.phase_spans.push(PhaseSpan {
                phase_id,
                start_time,
                end_time: Some(time),
            });
        }

        // Start tracking the new phase
        self.current_phase = Some((new_phase.to_string(), time));
    }

    /// Log phase end trigger - no longer prints inline
    pub fn phase_end_triggered(&mut self, _time: NaiveDateTime, _phase_id: &str) {
        // Phase end triggers are now implicit in the phase table
        // The actual transition happens in phase_change
    }

    /// Log counter change
    pub fn counter_change(&mut self, time: NaiveDateTime, counter_id: &str, old: u32, new: u32) {
        self.counter_changes += 1;
        if self.level < OutputLevel::Normal || !self.should_output() {
            return;
        }

        let time_str = self.format_time(time);
        let marker = self.bright_green("+++");
        let label = self.bright_green("COUNTER:");

        println!(
            "[{}] {} {} {} = {} → {}",
            time_str, marker, label, counter_id, old, new
        );
    }

    /// Log boss detection - prints buffered combat start
    pub fn boss_detected(&mut self, time: NaiveDateTime, boss_name: &str) {
        self.boss_detected_in_combat = true;

        // Print buffered combat start
        if let Some(start_time) = self.pending_combat_start.take() {
            if self.level >= OutputLevel::Normal {
                let label = self.bold(&self.green("═══ COMBAT START ═══"));
                println!("\n{}\n", label);
            }
            // Ensure combat_start is set for time formatting
            if self.combat_start.is_none() {
                self.combat_start = Some(start_time);
            }
        }

        if self.level < OutputLevel::Normal {
            return;
        }

        let time_str = self.format_time(time);
        let label = self.bold(&self.cyan("BOSS DETECTED:"));

        println!("[{}] {} {}", time_str, label, boss_name);
    }

    /// Log combat start - buffers until boss is detected
    pub fn combat_start(&mut self, time: NaiveDateTime) {
        self.set_combat_start(time);
        // Reset state for new combat
        self.boss_detected_in_combat = false;
        self.phase_spans.clear();
        self.current_phase = None;
        self.boss_hp.clear();
        // Buffer the combat start - will print when boss is detected
        self.pending_combat_start = Some(time);
    }

    /// Update boss HP for the current encounter
    pub fn update_boss_hp(&mut self, name: &str, npc_id: i64, current_hp: i64, max_hp: i64) {
        self.boss_hp.insert(
            npc_id,
            BossHpState {
                name: name.to_string(),
                npc_id,
                current_hp,
                max_hp,
            },
        );
    }

    /// Log combat end - only prints if boss was detected
    pub fn combat_end(
        &mut self,
        time: NaiveDateTime,
        duration_secs: f32,
        challenges: &[ChallengeValue],
    ) {
        // Finalize current phase if still active
        if let Some((phase_id, start_time)) = self.current_phase.take() {
            self.phase_spans.push(PhaseSpan {
                phase_id,
                start_time,
                end_time: Some(time),
            });
        }

        if self.level < OutputLevel::Normal || !self.should_output() {
            // Clear pending combat start if we're not outputting
            self.pending_combat_start = None;
            return;
        }

        // Print phase table for this fight
        if !self.phase_spans.is_empty() {
            self.print_phase_table();
        }

        // Print boss HP for this fight
        if !self.boss_hp.is_empty() {
            self.print_boss_hp_table();
        }

        // Print challenges for this fight
        if !challenges.is_empty() {
            self.print_challenge_table(challenges, duration_secs);
        }

        let time_str = self.format_time(time);
        let label = self.bold(&self.yellow("═══ COMBAT END ═══"));
        println!(
            "\n{} (duration: {:.1}s at {})\n",
            label, duration_secs, time_str
        );
    }

    /// Log a verbose debug message
    pub fn debug(&self, time: NaiveDateTime, msg: &str) {
        if self.level < OutputLevel::Verbose {
            return;
        }

        let time_str = self.format_time(time);
        let label = self.dim("DBG:");
        println!("[{}] {} {}", time_str, label, msg);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Summary Report
    // ═══════════════════════════════════════════════════════════════════════════

    /// Print final summary
    pub fn print_summary(&self, checkpoints_passed: Option<(u32, u32)>) {
        let line = "═".repeat(51);
        println!();
        println!("{}", line);
        println!("  TIMER VALIDATION SUMMARY");
        println!("{}", line);
        println!("Timers Started:  {}", self.timers_started);
        println!("Timers Expired:  {}", self.timers_expired);
        println!("Alerts Fired:    {}", self.alerts_fired);
        println!(
            "Phase Changes:   {}",
            if self.phase_changes > 0 {
                self.cyan(&self.phase_changes.to_string())
            } else {
                "0".to_string()
            }
        );
        println!(
            "Counter Updates: {}",
            if self.counter_changes > 0 {
                self.bright_green(&self.counter_changes.to_string())
            } else {
                "0".to_string()
            }
        );

        if let Some((passed, total)) = checkpoints_passed {
            let status = if passed == total {
                self.green(&format!("PASSED ({}/{})", passed, total))
            } else {
                self.red(&format!("FAILED ({}/{})", passed, total))
            };
            println!("Verification:    {}", status);
        }
        println!("{}", line);
    }

    /// Print the phase timing table
    fn print_phase_table(&self) {
        println!();
        println!("PHASES:");
        println!(
            "  {:25} {:>12} {:>12} {:>12}",
            "Phase", "Start", "End", "Duration"
        );
        println!("  {}", "─".repeat(65));

        for span in &self.phase_spans {
            let start_str = self.format_time_static(span.start_time);
            let end_str = span
                .end_time
                .map(|t| self.format_time_static(t))
                .unwrap_or_else(|| "-".to_string());

            let duration = span
                .end_time
                .map(|end| {
                    let secs = (end - span.start_time).num_milliseconds() as f32 / 1000.0;
                    format!("{:.1}s", secs)
                })
                .unwrap_or_else(|| "-".to_string());

            println!(
                "  {:25} {:>12} {:>12} {:>12}",
                truncate_phase(&span.phase_id, 25),
                start_str,
                end_str,
                duration
            );
        }
    }

    /// Print the boss HP table for the current encounter
    fn print_boss_hp_table(&self) {
        println!();
        println!("BOSS HP:");
        println!(
            "  {:30} {:>15} {:>10}",
            "Name", "HP", "%"
        );
        println!("  {}", "─".repeat(58));

        for state in self.boss_hp.values() {
            let pct = if state.max_hp > 0 {
                (state.current_hp as f64 / state.max_hp as f64) * 100.0
            } else {
                0.0
            };

            println!(
                "  {:30} {:>10}/{:<10} {:>6.1}%",
                truncate_phase(&state.name, 30),
                state.current_hp,
                state.max_hp,
                pct
            );
        }
    }

    /// Print the challenge table for the current encounter
    fn print_challenge_table(&self, challenges: &[ChallengeValue], duration_secs: f32) {
        println!();
        println!("CHALLENGES:");
        println!(
            "  {:25} {:>12} {:>8} {:>10}",
            "Name", "Value", "Events", "Per Sec"
        );
        println!("  {}", "─".repeat(58));

        for cv in challenges {
            let per_sec = if duration_secs > 0.0 {
                cv.value as f32 / duration_secs
            } else {
                0.0
            };
            let per_sec_str = if per_sec > 0.0 {
                format!("{:.1}/s", per_sec)
            } else {
                "-".to_string()
            };

            println!(
                "  {:25} {:>12} {:>8} {:>10}",
                truncate_phase(&cv.name, 25),
                format_number(cv.value),
                cv.event_count,
                per_sec_str
            );
        }
    }

    /// Format time without mutating self (for use in print_phase_table)
    fn format_time_static(&self, time: NaiveDateTime) -> String {
        if let Some(start) = self.combat_start {
            let delta = time - start;
            let secs = delta.num_milliseconds() as f32 / 1000.0;
            let mins = (secs / 60.0).floor() as u32;
            let secs_remainder = secs % 60.0;
            format!("{:02}:{:05.2}", mins, secs_remainder)
        } else {
            time.format("%H:%M:%S%.3f").to_string()
        }
    }

    /// Flush stdout
    pub fn flush(&self) {
        let _ = io::stdout().flush();
    }
}

fn truncate_phase(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_time(hour: u32, min: u32, sec: u32, ms: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2025, 1, 1)
            .unwrap()
            .and_hms_milli_opt(hour, min, sec, ms)
            .unwrap()
    }

    #[test]
    fn test_format_time_relative() {
        let mut output = CliOutput::new(OutputLevel::Normal);
        output.set_combat_start(make_time(12, 0, 0, 0));

        assert_eq!(output.format_time(make_time(12, 0, 15, 230)), "00:15.23");
        assert_eq!(output.format_time(make_time(12, 2, 45, 500)), "02:45.50");
    }

    #[test]
    fn test_quiet_suppresses_output() {
        let mut output = CliOutput::new(OutputLevel::Quiet);
        output.set_combat_start(make_time(12, 0, 0, 0));

        // These should increment counters but not print
        output.timer_start(make_time(12, 0, 10, 0), "Test", 15.0, "test_timer");
        output.alert(make_time(12, 0, 20, 0), "Test Alert", "");

        assert_eq!(output.timers_started, 1);
        assert_eq!(output.alerts_fired, 1);
    }
}
