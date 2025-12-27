//! System telemetry and pressure detection
//!
//! Samples CPU/memory/network at configurable rate.
//! Detects pressure spikes and triggers auto-throttle.

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use sysinfo::{System, Networks};

/// Telemetry sample
#[derive(Debug, Clone)]
pub struct Sample {
    pub timestamp: Instant,
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
}

/// Pressure level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl PressureLevel {
    pub fn from_cpu(cpu: f32) -> Self {
        if cpu > 90.0 { PressureLevel::Critical }
        else if cpu > 75.0 { PressureLevel::High }
        else if cpu > 50.0 { PressureLevel::Medium }
        else { PressureLevel::Low }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            PressureLevel::Low => "▁",
            PressureLevel::Medium => "▃",
            PressureLevel::High => "▆",
            PressureLevel::Critical => "█",
        }
    }
}

/// Telemetry ring buffer with pressure detection
pub struct Telemetry {
    samples: VecDeque<Sample>,
    max_samples: usize,
    system: System,
    networks: Networks,
    last_net_rx: u64,
    last_net_tx: u64,
    last_sample: Instant,

    /// Pre-spike snapshot (saved when pressure rises)
    pub spike_snapshot: Option<Vec<Sample>>,
}

impl Telemetry {
    pub fn new(window_seconds: usize, sample_rate_hz: u32) -> Self {
        let max_samples = window_seconds * sample_rate_hz as usize;

        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
            system: System::new_all(),
            networks: Networks::new_with_refreshed_list(),
            last_net_rx: 0,
            last_net_tx: 0,
            last_sample: Instant::now(),
            spike_snapshot: None,
        }
    }

    /// Take a new sample
    pub fn sample(&mut self) -> Sample {
        self.system.refresh_all();
        self.networks.refresh();

        // Calculate average CPU across all CPUs
        let cpus = self.system.cpus();
        let cpu_percent = if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
        };

        let total_mem = self.system.total_memory() as f32;
        let used_mem = self.system.used_memory() as f32;
        let mem_percent = if total_mem > 0.0 { (used_mem / total_mem) * 100.0 } else { 0.0 };

        let mut net_rx: u64 = 0;
        let mut net_tx: u64 = 0;
        for (_, data) in self.networks.iter() {
            net_rx += data.total_received();
            net_tx += data.total_transmitted();
        }

        // Calculate delta since last sample
        let rx_delta = net_rx.saturating_sub(self.last_net_rx);
        let tx_delta = net_tx.saturating_sub(self.last_net_tx);
        self.last_net_rx = net_rx;
        self.last_net_tx = net_tx;

        let sample = Sample {
            timestamp: Instant::now(),
            cpu_percent,
            mem_percent,
            net_rx_bytes: rx_delta,
            net_tx_bytes: tx_delta,
        };

        // Check for pressure spike
        if self.detect_spike(&sample) && self.spike_snapshot.is_none() {
            // Save pre-spike window
            self.spike_snapshot = Some(self.samples.iter().cloned().collect());
        }

        // Add to ring buffer
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample.clone());
        self.last_sample = Instant::now();

        sample
    }

    /// Check if current sample indicates a pressure spike
    fn detect_spike(&self, sample: &Sample) -> bool {
        // Spike if CPU jumps by more than 30% from recent average
        if let Some(avg_cpu) = self.average_cpu() {
            if sample.cpu_percent > avg_cpu + 30.0 {
                return true;
            }
        }

        // Or if CPU goes critical
        sample.cpu_percent > 90.0
    }

    /// Get average CPU over recent samples
    pub fn average_cpu(&self) -> Option<f32> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: f32 = self.samples.iter().map(|s| s.cpu_percent).sum();
        Some(sum / self.samples.len() as f32)
    }

    /// Get current pressure level
    pub fn pressure(&self) -> PressureLevel {
        self.samples.back()
            .map(|s| PressureLevel::from_cpu(s.cpu_percent))
            .unwrap_or(PressureLevel::Low)
    }

    /// Get recent samples for graphing
    pub fn recent(&self, count: usize) -> Vec<&Sample> {
        self.samples.iter().rev().take(count).collect()
    }

    /// Render a sparkline of CPU usage
    pub fn cpu_sparkline(&self, width: usize) -> String {
        let samples: Vec<_> = self.samples.iter().rev().take(width).collect();
        let mut chars = Vec::with_capacity(width);

        for sample in samples.iter().rev() {
            chars.push(PressureLevel::from_cpu(sample.cpu_percent).symbol());
        }

        // Pad if not enough samples
        while chars.len() < width {
            chars.insert(0, " ");
        }

        chars.join("")
    }

    /// Clear spike snapshot (after user acknowledges)
    pub fn clear_spike(&mut self) {
        self.spike_snapshot = None;
    }

    /// Time since last sample
    pub fn since_last_sample(&self) -> Duration {
        self.last_sample.elapsed()
    }
}

/// Throttle mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrottleMode {
    /// Full speed, no delays
    Full,
    /// Normal operation
    Normal,
    /// Throttled (longer delays between requests)
    Throttled,
    /// Killed (stop current operation)
    Killed,
}

impl ThrottleMode {
    /// Get delay multiplier for this mode
    pub fn delay_multiplier(&self) -> f32 {
        match self {
            ThrottleMode::Full => 0.0,
            ThrottleMode::Normal => 1.0,
            ThrottleMode::Throttled => 3.0,
            ThrottleMode::Killed => 0.0, // N/A
        }
    }

    /// Display name
    pub fn name(&self) -> &'static str {
        match self {
            ThrottleMode::Full => "FULL",
            ThrottleMode::Normal => "NORMAL",
            ThrottleMode::Throttled => "THROTTLED",
            ThrottleMode::Killed => "KILLED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_level() {
        assert_eq!(PressureLevel::from_cpu(10.0), PressureLevel::Low);
        assert_eq!(PressureLevel::from_cpu(60.0), PressureLevel::Medium);
        assert_eq!(PressureLevel::from_cpu(80.0), PressureLevel::High);
        assert_eq!(PressureLevel::from_cpu(95.0), PressureLevel::Critical);
    }

    #[test]
    fn test_telemetry_sample() {
        let mut tel = Telemetry::new(10, 1);
        let sample = tel.sample();
        assert!(sample.cpu_percent >= 0.0);
        assert!(sample.mem_percent >= 0.0);
    }

    #[test]
    fn test_throttle_mode() {
        assert_eq!(ThrottleMode::Normal.delay_multiplier(), 1.0);
        assert_eq!(ThrottleMode::Throttled.delay_multiplier(), 3.0);
    }
}
