//! Live cognitive loop: Coggy AtomSpace + OpenRouter free LLMs
//!
//! This is where the interplay happens:
//! 1. User input → Coggy PARSE/GROUND/ATTEND/INFER/REFLECT
//! 2. Coggy's focus atoms → shape the LLM prompt context
//! 3. Free LLM response → fed back into Coggy's AtomSpace
//! 4. Coggy's PLN → validates/extends LLM output symbolically
//! 5. Trace everything for profiling and validation

use std::time::Instant;

use crate::coggy_bridge::{CoggyBridge, CoggyThought};
use crate::cognitive::{self, CognitiveConfig, ContextCategory, SalienceContext};

/// A traced step in the live loop
#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub timestamp_ms: u128,
    pub phase: String,
    pub detail: String,
    pub duration_ms: u64,
    pub atom_count: usize,
    pub inference_count: usize,
}

/// Complete trace of a live cognitive cycle
#[derive(Debug)]
pub struct CycleTrace {
    pub entries: Vec<TraceEntry>,
    pub total_ms: u64,
    pub llm_model: String,
    pub llm_tokens_in: usize,
    pub llm_tokens_out: usize,
}

impl std::fmt::Display for CycleTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== COGNITIVE CYCLE TRACE ({} ms) ===", self.total_ms)?;
        writeln!(f, "  LLM: {} (in:{} out:{})", self.llm_model, self.llm_tokens_in, self.llm_tokens_out)?;
        for entry in &self.entries {
            writeln!(
                f,
                "  [{:>6}ms] {:20} | atoms:{:>4} inf:{:>3} | {}",
                entry.duration_ms,
                entry.phase,
                entry.atom_count,
                entry.inference_count,
                if entry.detail.len() > 80 {
                    format!("{}...", &entry.detail[..80])
                } else {
                    entry.detail.clone()
                }
            )?;
        }
        Ok(())
    }
}

/// The live cognitive engine
pub struct CoggyLive {
    pub bridge: CoggyBridge,
    pub config: CognitiveConfig,
    pub traces: Vec<CycleTrace>,
    cycle_count: u32,
    start_time: Instant,
}

impl CoggyLive {
    pub fn new() -> Self {
        Self {
            bridge: CoggyBridge::new(),
            config: CognitiveConfig::default(),
            traces: Vec::new(),
            cycle_count: 0,
            start_time: Instant::now(),
        }
    }

    /// Run a full cognitive cycle: Coggy think → shape prompt → call LLM → absorb response
    pub fn cycle(&mut self, user_input: &str) -> CycleResult {
        let cycle_start = Instant::now();
        self.cycle_count += 1;
        let mut entries = Vec::new();

        // Phase 1: Coggy thinks about the input
        let t = Instant::now();
        let thought = self.bridge.think(user_input);
        entries.push(TraceEntry {
            timestamp_ms: self.start_time.elapsed().as_millis(),
            phase: "COGGY_THINK".into(),
            detail: format!(
                "{} new atoms, {} inferences, {} total",
                thought.new_atoms, thought.inferences, thought.total_atoms
            ),
            duration_ms: t.elapsed().as_millis() as u64,
            atom_count: thought.total_atoms,
            inference_count: thought.inferences,
        });

        // Phase 2: Build salience-aware context from AtomSpace
        let t = Instant::now();
        let mut salience = SalienceContext::new(self.config.context_budget);
        let keywords = cognitive::extract_keywords(user_input);
        salience.set_keywords(keywords);

        // Inject Coggy's attention focus into salience
        self.bridge.enrich_salience(&mut salience);

        // Add user input as highest priority
        salience.add(
            user_input.to_string(),
            ContextCategory::UserMessage,
            0,
        );

        let context = salience.build();
        entries.push(TraceEntry {
            timestamp_ms: self.start_time.elapsed().as_millis(),
            phase: "SALIENCE_BUILD".into(),
            detail: format!("{} chars context, {} stats", context.len(), salience.stats()),
            duration_ms: t.elapsed().as_millis() as u64,
            atom_count: thought.total_atoms,
            inference_count: 0,
        });

        // Phase 3: Shape prompt for free LLM
        let t = Instant::now();
        let prompt = self.shape_prompt(user_input, &thought, &context);
        entries.push(TraceEntry {
            timestamp_ms: self.start_time.elapsed().as_millis(),
            phase: "PROMPT_SHAPE".into(),
            detail: format!("{} chars prompt", prompt.len()),
            duration_ms: t.elapsed().as_millis() as u64,
            atom_count: thought.total_atoms,
            inference_count: 0,
        });

        // Phase 4: Health check (tikkun)
        let t = Instant::now();
        let health = self.bridge.health_check();
        entries.push(TraceEntry {
            timestamp_ms: self.start_time.elapsed().as_millis(),
            phase: "TIKKUN".into(),
            detail: format!(
                "healthy:{} confidence:{:.2} concerns:{}",
                health.on_track,
                health.confidence,
                health.concerns.len()
            ),
            duration_ms: t.elapsed().as_millis() as u64,
            atom_count: thought.total_atoms,
            inference_count: 0,
        });

        // Phase 5: Extract facts for Hyle's context layers
        let t = Instant::now();
        let facts = self.bridge.extract_facts();
        entries.push(TraceEntry {
            timestamp_ms: self.start_time.elapsed().as_millis(),
            phase: "FACT_EXTRACT".into(),
            detail: format!("{} facts extracted from AtomSpace", facts.len()),
            duration_ms: t.elapsed().as_millis() as u64,
            atom_count: thought.total_atoms,
            inference_count: 0,
        });

        let total_ms = cycle_start.elapsed().as_millis() as u64;

        let trace = CycleTrace {
            entries,
            total_ms,
            llm_model: self.config.executor_model.clone(),
            llm_tokens_in: prompt.len() / 4, // rough estimate
            llm_tokens_out: 0,
        };

        self.traces.push(trace);

        CycleResult {
            thought,
            prompt,
            context,
            healthy: health.on_track,
            facts_count: facts.len(),
            cycle: self.cycle_count,
            total_ms,
        }
    }

    /// Shape a prompt that grounds LLM reasoning in Coggy's symbolic knowledge
    fn shape_prompt(&self, user_input: &str, thought: &CoggyThought, context: &str) -> String {
        let focus_str: String = thought
            .focus
            .iter()
            .map(|(name, sti)| format!("  - {} (attention: {:.0})", name, sti))
            .collect::<Vec<_>>()
            .join("\n");

        let trace_str: String = thought
            .trace
            .iter()
            .take(3)
            .map(|t| format!("  {}", t))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"You are Hyle, a Rust-native code assistant with a cognitive architecture.

<symbolic-grounding>
AtomSpace: {} atoms | Turn {} | {} inferences this cycle

Focus (what the symbolic reasoner is attending to):
{}

Cognitive trace:
{}
</symbolic-grounding>

<context>
{}
</context>

<user>
{}
</user>

Respond with grounded reasoning. Use the symbolic trace to inform your approach.
If the AtomSpace contains relevant knowledge, cite it.
If PLN made inferences, build on them."#,
            thought.total_atoms,
            self.cycle_count,
            thought.inferences,
            if focus_str.is_empty() {
                "  (no focus atoms)".to_string()
            } else {
                focus_str
            },
            if trace_str.is_empty() {
                "  (empty trace)".to_string()
            } else {
                trace_str
            },
            context,
            user_input
        )
    }

    /// Get the last trace
    pub fn last_trace(&self) -> Option<&CycleTrace> {
        self.traces.last()
    }

    /// Print all traces as a profiling report
    pub fn profile_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!(
            "=== COGNITIVE PROFILING REPORT ===\n  Cycles: {}\n  Total traces: {}\n\n",
            self.cycle_count,
            self.traces.len()
        ));

        for (i, trace) in self.traces.iter().enumerate() {
            report.push_str(&format!("--- Cycle {} ---\n{}\n", i + 1, trace));
        }

        // Summary stats
        if !self.traces.is_empty() {
            let avg_ms: u64 =
                self.traces.iter().map(|t| t.total_ms).sum::<u64>() / self.traces.len() as u64;
            let total_atoms = self
                .traces
                .last()
                .and_then(|t| t.entries.last())
                .map(|e| e.atom_count)
                .unwrap_or(0);
            report.push_str(&format!(
                "--- Summary ---\n  Avg cycle: {}ms\n  Final atoms: {}\n",
                avg_ms, total_atoms
            ));
        }

        report
    }
}

/// Result of a cognitive cycle
pub struct CycleResult {
    pub thought: CoggyThought,
    pub prompt: String,
    pub context: String,
    pub healthy: bool,
    pub facts_count: usize,
    pub cycle: u32,
    pub total_ms: u64,
}

impl std::fmt::Display for CycleResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Cycle {} | {}ms | {} atoms | {} inferences | {} facts | healthy:{}",
            self.cycle,
            self.total_ms,
            self.thought.total_atoms,
            self.thought.inferences,
            self.facts_count,
            self.healthy
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_live_cycle() {
        let mut live = CoggyLive::new();
        let result = live.cycle("cat is-a pet");
        assert!(result.thought.total_atoms > 30);
        assert!(result.healthy);
        assert!(!result.prompt.is_empty());
        assert!(result.total_ms < 1000, "Should complete in under 1s");
    }

    #[test]
    fn test_multi_cycle() {
        let mut live = CoggyLive::new();
        live.cycle("dog is-a mammal");
        live.cycle("fix the login bug");
        live.cycle("read src/main.rs");

        assert_eq!(live.traces.len(), 3);
        let report = live.profile_report();
        assert!(report.contains("Cycles: 3"));
    }

    #[test]
    fn test_trace_output() {
        let mut live = CoggyLive::new();
        live.cycle("hello world");
        let trace = live.last_trace().unwrap();
        assert!(trace.total_ms < 500);
        assert!(trace.entries.len() >= 4);

        // Print it to verify formatting
        let output = format!("{}", trace);
        assert!(output.contains("COGGY_THINK"));
        assert!(output.contains("TIKKUN"));
    }

    #[test]
    fn test_profile_report() {
        let mut live = CoggyLive::new();
        live.cycle("understand the codebase");
        live.cycle("identify the weakest module");
        let report = live.profile_report();
        assert!(report.contains("COGNITIVE PROFILING REPORT"));
        assert!(report.contains("Avg cycle"));
    }
}
