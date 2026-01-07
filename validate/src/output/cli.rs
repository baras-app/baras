//! Colored CLI output for timer events
//!
//! Formats timer starts, stops, alerts, and phase changes with
//! colored output for easy visual parsing.

use chrono::NaiveDateTime;
use std::io::{self, Write};

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
        }
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
        if self.level < OutputLevel::Normal {
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
        if self.level < OutputLevel::Normal {
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
        if self.level < OutputLevel::Normal {
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
        if self.level < OutputLevel::Normal {
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
        if self.level < OutputLevel::Normal {
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

    /// Log phase change
    pub fn phase_change(&mut self, time: NaiveDateTime, old_phase: Option<&str>, new_phase: &str) {
        self.phase_changes += 1;
        if self.level < OutputLevel::Normal {
            return;
        }

        let time_str = self.format_time(time);
        let marker = self.cyan("~~~");
        let label = self.cyan("PHASE:");

        if let Some(old) = old_phase {
            println!(
                "[{}] {} {} {} → {}",
                time_str,
                marker,
                label,
                old,
                self.bold(new_phase)
            );
        } else {
            println!(
                "[{}] {} {} → {} (initial)",
                time_str,
                marker,
                label,
                self.bold(new_phase)
            );
        }
    }

    /// Log phase end trigger (end_trigger fired, emits PhaseEndTriggered signal)
    pub fn phase_end_triggered(&mut self, time: NaiveDateTime, phase_id: &str) {
        if self.level < OutputLevel::Normal {
            return;
        }

        let time_str = self.format_time(time);
        let marker = self.yellow("<<<");
        let label = self.yellow("PHASE END TRIGGERED:");
        println!("[{}] {} {} {}", time_str, marker, label, phase_id);
    }

    /// Log counter change
    pub fn counter_change(&mut self, time: NaiveDateTime, counter_id: &str, old: u32, new: u32) {
        self.counter_changes += 1;
        if self.level < OutputLevel::Normal {
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

    /// Log boss detection
    pub fn boss_detected(&mut self, time: NaiveDateTime, boss_name: &str) {
        if self.level < OutputLevel::Normal {
            return;
        }

        let time_str = self.format_time(time);
        let label = self.bold(&self.cyan("BOSS DETECTED:"));

        println!("[{}] {} {}", time_str, label, boss_name);
    }

    /// Log combat start
    pub fn combat_start(&mut self, time: NaiveDateTime) {
        self.set_combat_start(time);
        if self.level < OutputLevel::Normal {
            return;
        }

        let label = self.bold(&self.green("═══ COMBAT START ═══"));
        println!("\n{}\n", label);
    }

    /// Log combat end
    pub fn combat_end(&mut self, time: NaiveDateTime, duration_secs: f32) {
        if self.level < OutputLevel::Normal {
            return;
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

    /// Flush stdout
    pub fn flush(&self) {
        let _ = io::stdout().flush();
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
