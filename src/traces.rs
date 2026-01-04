//! Telemetry traces for tokens, context, and memory
//!
//! Provides ring buffers for tracking:
//! - Token usage over time
//! - Context window utilization
//! - Memory pressure
//! - Request latency

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A single trace sample
#[derive(Debug, Clone)]
pub struct TraceSample {
    #[allow(dead_code)] // Used by last_sample_age()
    pub timestamp: Instant,
    pub value: f64,
}

/// Ring buffer for trace data
#[derive(Debug)]
pub struct TraceBuffer {
    samples: VecDeque<TraceSample>,
    max_samples: usize,
    pub label: String,
    pub unit: String,
}

impl TraceBuffer {
    pub fn new(label: &str, unit: &str, max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
            label: label.to_string(),
            unit: unit.to_string(),
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(TraceSample {
            timestamp: Instant::now(),
            value,
        });
    }

    pub fn last(&self) -> Option<f64> {
        self.samples.back().map(|s| s.value)
    }

    /// Get age of most recent sample
    #[allow(dead_code)] // Utility for debugging
    pub fn last_sample_age(&self) -> Option<std::time::Duration> {
        self.samples.back().map(|s| s.timestamp.elapsed())
    }

    pub fn average(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: f64 = self.samples.iter().map(|s| s.value).sum();
        Some(sum / self.samples.len() as f64)
    }

    pub fn max(&self) -> Option<f64> {
        self.samples
            .iter()
            .map(|s| s.value)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
    }

    pub fn sparkline(&self, width: usize) -> String {
        const BARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

        let samples: Vec<f64> = self
            .samples
            .iter()
            .rev()
            .take(width)
            .map(|s| s.value)
            .collect();
        if samples.is_empty() {
            return " ".repeat(width);
        }

        let max = samples.iter().cloned().fold(f64::MIN, f64::max).max(1.0);
        let min = samples.iter().cloned().fold(f64::MAX, f64::min).min(0.0);
        let range = (max - min).max(1.0);

        let mut result = String::with_capacity(width);
        for v in samples.iter().rev() {
            let normalized = ((v - min) / range).clamp(0.0, 1.0);
            let idx = (normalized * (BARS.len() - 1) as f64).round() as usize;
            result.push(BARS[idx]);
        }

        // Pad if not enough samples
        while result.chars().count() < width {
            result.insert(0, ' ');
        }

        result
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Token usage trace
#[derive(Debug)]
pub struct TokenTrace {
    pub prompt_tokens: TraceBuffer,
    pub completion_tokens: TraceBuffer,
    pub tokens_per_sec: TraceBuffer,
    pub total_prompt: u64,
    pub total_completion: u64,
}

impl TokenTrace {
    pub fn new(max_samples: usize) -> Self {
        Self {
            prompt_tokens: TraceBuffer::new("Prompt", "tok", max_samples),
            completion_tokens: TraceBuffer::new("Completion", "tok", max_samples),
            tokens_per_sec: TraceBuffer::new("Rate", "tok/s", max_samples),
            total_prompt: 0,
            total_completion: 0,
        }
    }

    pub fn record(&mut self, prompt: u32, completion: u32, duration_secs: f64) {
        self.prompt_tokens.push(prompt as f64);
        self.completion_tokens.push(completion as f64);
        self.total_prompt += prompt as u64;
        self.total_completion += completion as u64;

        if duration_secs > 0.0 {
            self.tokens_per_sec.push(completion as f64 / duration_secs);
        }
    }

    pub fn total(&self) -> u64 {
        self.total_prompt + self.total_completion
    }
}

/// Context window trace
#[derive(Debug)]
pub struct ContextTrace {
    pub usage: TraceBuffer,
    pub context_window: u32,
    pub warn_threshold: f64,
}

impl ContextTrace {
    pub fn new(context_window: u32, max_samples: usize) -> Self {
        Self {
            usage: TraceBuffer::new("Context", "%", max_samples),
            context_window,
            warn_threshold: 0.8,
        }
    }

    pub fn record(&mut self, tokens_used: u32) {
        let ratio = tokens_used as f64 / self.context_window as f64 * 100.0;
        self.usage.push(ratio);
    }

    pub fn is_warning(&self) -> bool {
        self.usage
            .last()
            .map(|v| v / 100.0 > self.warn_threshold)
            .unwrap_or(false)
    }

    pub fn is_full(&self) -> bool {
        self.usage.last().map(|v| v >= 100.0).unwrap_or(false)
    }
}

/// Memory trace (RSS)
#[derive(Debug)]
pub struct MemoryTrace {
    pub rss: TraceBuffer,
    pub heap: TraceBuffer,
}

impl MemoryTrace {
    pub fn new(max_samples: usize) -> Self {
        Self {
            rss: TraceBuffer::new("RSS", "MB", max_samples),
            heap: TraceBuffer::new("Heap", "MB", max_samples),
        }
    }

    pub fn sample(&mut self) {
        // Get current process memory
        if let Ok(mem) = get_process_memory() {
            self.rss.push(mem.rss_mb);
            self.heap.push(mem.heap_mb);
        }
    }
}

/// Process memory info
struct ProcessMemory {
    rss_mb: f64,
    heap_mb: f64,
}

fn get_process_memory() -> std::io::Result<ProcessMemory> {
    // Read from /proc/self/statm on Linux
    let statm = std::fs::read_to_string("/proc/self/statm")?;
    let parts: Vec<&str> = statm.split_whitespace().collect();

    let page_size = 4096.0; // Assume 4KB pages
    let rss_pages: f64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let rss_mb = rss_pages * page_size / 1024.0 / 1024.0;

    // Heap is harder to get, use data segment as approximation
    let data_pages: f64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let heap_mb = data_pages * page_size / 1024.0 / 1024.0;

    Ok(ProcessMemory { rss_mb, heap_mb })
}

/// Request latency trace
#[derive(Debug)]
pub struct LatencyTrace {
    pub ttft: TraceBuffer,  // Time to first token
    pub total: TraceBuffer, // Total request time
}

impl LatencyTrace {
    pub fn new(max_samples: usize) -> Self {
        Self {
            ttft: TraceBuffer::new("TTFT", "ms", max_samples),
            total: TraceBuffer::new("Total", "ms", max_samples),
        }
    }

    pub fn record_ttft(&mut self, duration: Duration) {
        self.ttft.push(duration.as_millis() as f64);
    }

    pub fn record_total(&mut self, duration: Duration) {
        self.total.push(duration.as_millis() as f64);
    }
}

/// All traces combined
#[derive(Debug)]
pub struct Traces {
    pub tokens: TokenTrace,
    pub context: ContextTrace,
    pub memory: MemoryTrace,
    pub latency: LatencyTrace,
}

impl Traces {
    pub fn new(context_window: u32) -> Self {
        let max_samples = 120; // 2 minutes at 1Hz

        Self {
            tokens: TokenTrace::new(max_samples),
            context: ContextTrace::new(context_window, max_samples),
            memory: MemoryTrace::new(max_samples),
            latency: LatencyTrace::new(max_samples),
        }
    }

    /// Render traces as multi-line summary using buffer labels and units
    pub fn render(&self, width: usize) -> Vec<String> {
        let sw = width.saturating_sub(20).min(30);

        // Helper to render a buffer with its metadata
        let render_buf = |buf: &TraceBuffer, extra: &str| -> String {
            format!(
                "{}: {} [{} {} samples]{}",
                buf.label,
                buf.sparkline(sw),
                buf.len(),
                buf.unit,
                extra
            )
        };

        vec![
            format!(
                "{}: {} [{:>6} total, {} samples]",
                self.tokens.tokens_per_sec.label,
                self.tokens.tokens_per_sec.sparkline(sw),
                format_count(self.tokens.total()),
                self.tokens.tokens_per_sec.len()
            ),
            format!(
                "{}: {} [{:>5.1}%, {} samples]",
                self.context.usage.label,
                self.context.usage.sparkline(sw),
                self.context.usage.last().unwrap_or(0.0),
                self.context.usage.len()
            ),
            render_buf(
                &self.memory.rss,
                &format!(" [{:.1} MB]", self.memory.rss.last().unwrap_or(0.0)),
            ),
            render_buf(
                &self.latency.ttft,
                &format!(" [{:.0}ms]", self.latency.ttft.last().unwrap_or(0.0)),
            ),
        ]
    }

    /// Check if any traces have data
    pub fn has_data(&self) -> bool {
        !self.tokens.tokens_per_sec.is_empty()
            || !self.context.usage.is_empty()
            || !self.memory.rss.is_empty()
            || !self.latency.ttft.is_empty()
    }
}

fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_buffer() {
        let mut buf = TraceBuffer::new("Test", "units", 10);
        assert!(buf.is_empty());

        buf.push(1.0);
        buf.push(2.0);
        buf.push(3.0);

        assert_eq!(buf.len(), 3);
        assert_eq!(buf.last(), Some(3.0));
        assert_eq!(buf.average(), Some(2.0));
        assert_eq!(buf.max(), Some(3.0));
    }

    #[test]
    fn test_sparkline() {
        let mut buf = TraceBuffer::new("Test", "units", 10);
        for i in 0..10 {
            buf.push(i as f64 * 10.0);
        }

        let spark = buf.sparkline(10);
        assert_eq!(spark.chars().count(), 10);
        assert!(spark.contains('▁')); // Low value
        assert!(spark.contains('█')); // High value
    }

    #[test]
    fn test_token_trace() {
        let mut trace = TokenTrace::new(10);
        trace.record(100, 50, 1.0);

        assert_eq!(trace.total(), 150);
        assert_eq!(trace.tokens_per_sec.last(), Some(50.0));
    }
}
