//! UX Metrics - Measuring user experience quality
//!
//! This module provides metrics for evaluating hyle's usability:
//! - Responsiveness: how quickly does the system respond to input
//! - Smoothness: how consistent is the streaming/rendering
//! - Autonomy: how much can hyle accomplish without intervention
//!
//! These metrics are hard to test automatically but crucial for trust.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════
// RESPONSIVENESS METRICS
// ═══════════════════════════════════════════════════════════════

/// Track input-to-response latency
#[derive(Debug, Default)]
pub struct ResponsivenessTracker {
    /// Time from user input to first visible response
    input_latencies: VecDeque<Duration>,
    /// Time from sending request to first token
    first_token_latencies: VecDeque<Duration>,
    /// Maximum samples to keep
    max_samples: usize,
}

impl ResponsivenessTracker {
    pub fn new(max_samples: usize) -> Self {
        Self {
            input_latencies: VecDeque::with_capacity(max_samples),
            first_token_latencies: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }

    pub fn record_input_latency(&mut self, latency: Duration) {
        if self.input_latencies.len() >= self.max_samples {
            self.input_latencies.pop_front();
        }
        self.input_latencies.push_back(latency);
    }

    pub fn record_first_token(&mut self, latency: Duration) {
        if self.first_token_latencies.len() >= self.max_samples {
            self.first_token_latencies.pop_front();
        }
        self.first_token_latencies.push_back(latency);
    }

    /// Calculate percentiles: p50, p95, p99
    pub fn input_percentiles(&self) -> Percentiles {
        calculate_percentiles(&self.input_latencies)
    }

    pub fn first_token_percentiles(&self) -> Percentiles {
        calculate_percentiles(&self.first_token_latencies)
    }

    /// Is the system responsive? (p95 under threshold)
    pub fn is_responsive(&self, threshold_ms: u64) -> bool {
        self.input_percentiles().p95.as_millis() < threshold_ms as u128
    }
}

#[derive(Debug, Clone, Default)]
pub struct Percentiles {
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
}

fn calculate_percentiles(samples: &VecDeque<Duration>) -> Percentiles {
    if samples.is_empty() {
        return Percentiles::default();
    }

    let mut sorted: Vec<Duration> = samples.iter().copied().collect();
    sorted.sort();

    let len = sorted.len();
    Percentiles {
        p50: sorted[len / 2],
        p95: sorted[(len * 95) / 100],
        p99: sorted[(len * 99) / 100],
    }
}

// ═══════════════════════════════════════════════════════════════
// STREAMING SMOOTHNESS
// ═══════════════════════════════════════════════════════════════

/// Track streaming consistency
#[derive(Debug)]
pub struct SmoothnessTracker {
    /// Inter-token arrival times
    token_intervals: VecDeque<Duration>,
    last_token_time: Option<Instant>,
    max_samples: usize,
}

impl SmoothnessTracker {
    pub fn new(max_samples: usize) -> Self {
        Self {
            token_intervals: VecDeque::with_capacity(max_samples),
            last_token_time: None,
            max_samples,
        }
    }

    pub fn record_token(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_token_time {
            let interval = now - last;
            if self.token_intervals.len() >= self.max_samples {
                self.token_intervals.pop_front();
            }
            self.token_intervals.push_back(interval);
        }
        self.last_token_time = Some(now);
    }

    pub fn reset(&mut self) {
        self.last_token_time = None;
    }

    /// Calculate jitter (variance in token arrival)
    /// Low jitter = smooth streaming
    pub fn jitter(&self) -> Duration {
        if self.token_intervals.len() < 2 {
            return Duration::ZERO;
        }

        let mean: f64 = self.token_intervals.iter().map(|d| d.as_secs_f64()).sum::<f64>()
            / self.token_intervals.len() as f64;

        let variance: f64 = self
            .token_intervals
            .iter()
            .map(|d| (d.as_secs_f64() - mean).powi(2))
            .sum::<f64>()
            / self.token_intervals.len() as f64;

        Duration::from_secs_f64(variance.sqrt())
    }

    /// Tokens per second (smoothed)
    pub fn tokens_per_second(&self) -> f64 {
        if self.token_intervals.is_empty() {
            return 0.0;
        }

        let total_time: Duration = self.token_intervals.iter().sum();
        if total_time.is_zero() {
            return 0.0;
        }

        self.token_intervals.len() as f64 / total_time.as_secs_f64()
    }

    /// Is streaming smooth? (low jitter relative to mean interval)
    pub fn is_smooth(&self, max_jitter_ratio: f64) -> bool {
        if self.token_intervals.is_empty() {
            return true;
        }

        let mean: f64 = self.token_intervals.iter().map(|d| d.as_secs_f64()).sum::<f64>()
            / self.token_intervals.len() as f64;

        let jitter = self.jitter().as_secs_f64();
        jitter / mean < max_jitter_ratio
    }
}

// ═══════════════════════════════════════════════════════════════
// AUTONOMY METRICS
// ═══════════════════════════════════════════════════════════════

/// Track agent autonomy and task completion
#[derive(Debug, Default)]
pub struct AutonomyTracker {
    /// Tasks started
    pub tasks_started: usize,
    /// Tasks completed without intervention
    pub tasks_completed_autonomous: usize,
    /// Tasks requiring user help
    pub tasks_required_help: usize,
    /// Tasks that got stuck
    pub tasks_stuck: usize,
    /// Total iterations across all tasks
    pub total_iterations: usize,
    /// Iterations per completed task
    iterations_per_task: VecDeque<usize>,
}

impl AutonomyTracker {
    pub fn new() -> Self {
        Self {
            iterations_per_task: VecDeque::with_capacity(100),
            ..Default::default()
        }
    }

    pub fn start_task(&mut self) {
        self.tasks_started += 1;
    }

    pub fn complete_task(&mut self, iterations: usize, required_help: bool) {
        self.total_iterations += iterations;
        if self.iterations_per_task.len() >= 100 {
            self.iterations_per_task.pop_front();
        }
        self.iterations_per_task.push_back(iterations);

        if required_help {
            self.tasks_required_help += 1;
        } else {
            self.tasks_completed_autonomous += 1;
        }
    }

    pub fn task_stuck(&mut self, iterations: usize) {
        self.tasks_stuck += 1;
        self.total_iterations += iterations;
    }

    /// Autonomous completion rate (0.0 - 1.0)
    pub fn autonomy_rate(&self) -> f64 {
        let completed = self.tasks_completed_autonomous + self.tasks_required_help;
        if completed == 0 {
            return 0.0;
        }
        self.tasks_completed_autonomous as f64 / completed as f64
    }

    /// Average iterations to complete a task
    pub fn avg_iterations(&self) -> f64 {
        if self.iterations_per_task.is_empty() {
            return 0.0;
        }
        self.iterations_per_task.iter().sum::<usize>() as f64
            / self.iterations_per_task.len() as f64
    }

    /// Success rate (completed / started)
    pub fn success_rate(&self) -> f64 {
        if self.tasks_started == 0 {
            return 0.0;
        }
        (self.tasks_completed_autonomous + self.tasks_required_help) as f64
            / self.tasks_started as f64
    }

    /// Stuck rate (stuck / started)
    pub fn stuck_rate(&self) -> f64 {
        if self.tasks_started == 0 {
            return 0.0;
        }
        self.tasks_stuck as f64 / self.tasks_started as f64
    }
}

// ═══════════════════════════════════════════════════════════════
// UX QUALITY SCORE
// ═══════════════════════════════════════════════════════════════

/// Overall UX quality assessment
#[derive(Debug)]
pub struct UxQuality {
    pub responsiveness: ResponsivenessTracker,
    pub smoothness: SmoothnessTracker,
    pub autonomy: AutonomyTracker,
}

impl UxQuality {
    pub fn new() -> Self {
        Self {
            responsiveness: ResponsivenessTracker::new(100),
            smoothness: SmoothnessTracker::new(1000),
            autonomy: AutonomyTracker::new(),
        }
    }

    /// Overall quality score (0-100)
    /// Weights: responsiveness 30%, smoothness 30%, autonomy 40%
    pub fn score(&self) -> u8 {
        let resp_score = if self.responsiveness.is_responsive(200) {
            100.0
        } else if self.responsiveness.is_responsive(500) {
            70.0
        } else if self.responsiveness.is_responsive(1000) {
            40.0
        } else {
            20.0
        };

        let smooth_score = if self.smoothness.is_smooth(0.5) {
            100.0
        } else if self.smoothness.is_smooth(1.0) {
            70.0
        } else {
            40.0
        };

        let autonomy_score = self.autonomy.autonomy_rate() * 100.0;

        let weighted = resp_score * 0.3 + smooth_score * 0.3 + autonomy_score * 0.4;
        weighted.min(100.0) as u8
    }

    /// Summary for display
    pub fn summary(&self) -> String {
        format!(
            "UX Quality: {}/100\n\
             ├─ Responsiveness: p95={}ms\n\
             ├─ Smoothness: {:.1} tok/s, jitter={}ms\n\
             └─ Autonomy: {:.0}% ({}/{} tasks)",
            self.score(),
            self.responsiveness.input_percentiles().p95.as_millis(),
            self.smoothness.tokens_per_second(),
            self.smoothness.jitter().as_millis(),
            self.autonomy.autonomy_rate() * 100.0,
            self.autonomy.tasks_completed_autonomous,
            self.autonomy.tasks_started,
        )
    }
}

impl Default for UxQuality {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_responsiveness_percentiles() {
        let mut tracker = ResponsivenessTracker::new(100);

        // Add samples: 10, 20, 30, ..., 100 ms
        for i in 1..=10 {
            tracker.record_input_latency(Duration::from_millis(i * 10));
        }

        let p = tracker.input_percentiles();
        assert!(p.p50 >= Duration::from_millis(50));
        assert!(p.p95 >= Duration::from_millis(90));
    }

    #[test]
    fn test_smoothness_jitter() {
        let mut tracker = SmoothnessTracker::new(100);

        // Uniform intervals = low jitter
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(10));
            tracker.record_token();
        }

        // Jitter should be low for uniform intervals
        let jitter = tracker.jitter();
        assert!(jitter < Duration::from_millis(10), "Jitter should be low for uniform intervals");
    }

    #[test]
    fn test_autonomy_rate() {
        let mut tracker = AutonomyTracker::new();

        tracker.start_task();
        tracker.complete_task(5, false); // autonomous

        tracker.start_task();
        tracker.complete_task(10, true); // required help

        tracker.start_task();
        tracker.task_stuck(3); // stuck

        assert_eq!(tracker.autonomy_rate(), 0.5); // 1/2 autonomous
        assert_eq!(tracker.success_rate(), 2.0 / 3.0); // 2/3 completed
        assert!(tracker.stuck_rate() > 0.3); // 1/3 stuck
    }

    #[test]
    fn test_ux_quality_score() {
        let quality = UxQuality::new();
        // Empty trackers should give a reasonable baseline
        let score = quality.score();
        assert!(score <= 100);
    }

    #[test]
    fn test_smoothness_is_smooth() {
        let mut tracker = SmoothnessTracker::new(100);

        // No tokens = smooth by default
        assert!(tracker.is_smooth(0.5));

        // Add some uniform tokens
        for _ in 0..5 {
            tracker.record_token();
            std::thread::sleep(Duration::from_millis(5));
        }

        // Should still be smooth
        assert!(tracker.is_smooth(2.0));
    }
}
