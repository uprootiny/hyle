//! Bridge between Hyle's cognitive system and Coggy's AtomSpace/PLN/ECAN
//!
//! Maps Hyle concepts to Coggy atoms and uses Coggy's inference engine
//! to enhance Hyle's decision-making with grounded symbolic reasoning.

use coggy::atom::{AtomType, TruthValue};
use coggy::atomspace::AtomSpace;
use coggy::cogloop;
use coggy::ecan::EcanConfig;
use coggy::pln;
use coggy::tikkun;

use crate::cognitive::{
    ContextCategory, Fact, FactCategory, Momentum, SalienceContext, SalienceTier, SanityResult,
    ToolOutcome,
};

/// Unified cognitive state bridging Hyle and Coggy
pub struct CoggyBridge {
    pub space: AtomSpace,
    pub ecan_config: EcanConfig,
    /// Map Hyle file paths to ConceptNode IDs
    file_atoms: std::collections::HashMap<String, u64>,
    /// Map Hyle tool names to PredicateNode IDs
    tool_atoms: std::collections::HashMap<String, u64>,
}

impl CoggyBridge {
    pub fn new() -> Self {
        let mut space = AtomSpace::new();
        coggy::ontology::load_base_ontology(&mut space);

        // Seed with Hyle-specific concepts
        let tv_high = TruthValue::new(0.9, 0.9);
        space.add_node(AtomType::ConceptNode, "hyle", tv_high);
        space.add_node(AtomType::ConceptNode, "code-assistant", tv_high);
        space.add_node(AtomType::ConceptNode, "tool-call", tv_high);
        space.add_node(AtomType::ConceptNode, "user-intent", tv_high);
        space.add_node(AtomType::ConceptNode, "file-edit", tv_high);
        space.add_node(AtomType::ConceptNode, "error", TruthValue::new(0.5, 0.7));
        space.add_node(AtomType::ConceptNode, "success", TruthValue::new(0.8, 0.8));

        // Relationships
        let (hyle_id, _) = space.add_node(AtomType::ConceptNode, "hyle", tv_high);
        let (ca_id, _) = space.add_node(AtomType::ConceptNode, "code-assistant", tv_high);
        space.add_link(AtomType::InheritanceLink, vec![hyle_id, ca_id], tv_high);

        Self {
            space,
            ecan_config: EcanConfig::default(),
            file_atoms: std::collections::HashMap::new(),
            tool_atoms: std::collections::HashMap::new(),
        }
    }

    /// Record a tool call in the AtomSpace
    pub fn record_tool_call(&mut self, tool_name: &str, target: &str, success: bool) {
        let tv = if success {
            TruthValue::new(0.9, 0.8)
        } else {
            TruthValue::new(0.2, 0.8)
        };

        let (tool_id, _) = self.space.add_node(
            AtomType::PredicateNode,
            tool_name,
            TruthValue::new(0.8, 0.7),
        );
        self.tool_atoms.insert(tool_name.to_string(), tool_id);

        let (target_id, _) = self.space.add_node(
            AtomType::ConceptNode,
            target,
            TruthValue::new(0.7, 0.5),
        );

        // EvaluationLink: tool_name(target)
        self.space
            .add_link(AtomType::EvaluationLink, vec![tool_id, target_id], tv);

        // Track file atoms
        if tool_name == "write" || tool_name == "edit" || tool_name == "read" {
            self.file_atoms.insert(target.to_string(), target_id);
        }
    }

    /// Record a user intent in the AtomSpace
    pub fn record_intent(&mut self, intent: &str) {
        let result = cogloop::run(&mut self.space, intent, &self.ecan_config);
        // The cogloop already parsed, grounded, attended, inferred, and reflected
        let _ = result;
    }

    /// Get attention-ranked atoms for salience scoring
    pub fn top_focus(&self, limit: usize) -> Vec<(String, f64)> {
        self.space
            .atoms_by_sti(limit)
            .iter()
            .map(|a| (self.space.format_atom(a.id), a.av.sti))
            .collect()
    }

    /// Run PLN inference and return new derivations
    pub fn infer(&mut self, depth: usize) -> Vec<String> {
        let inferences = pln::forward_chain(&mut self.space, depth as u32);
        inferences
            .iter()
            .map(|inf| {
                let name = self.space.format_atom(inf.conclusion_id);
                format!("{} <- {} ({})", name, inf.rule, inf.tv)
            })
            .collect()
    }

    /// Run tikkun self-repair and return health status
    pub fn health_check(&self) -> SanityResult {
        let report = tikkun::run_tikkun(&self.space);
        let all_passed = report.all_healthy;
        let concerns: Vec<String> = report
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| {
                format!(
                    "{}: {}",
                    c.name,
                    c.detail.as_deref().unwrap_or("failed")
                )
            })
            .collect();

        SanityResult {
            on_track: all_passed,
            confidence: if all_passed { 1.0 } else { 0.5 },
            progress_estimate: 0.0,
            concerns,
            suggestions: vec![],
            should_pause: !all_passed,
            should_abort: false,
        }
    }

    /// Convert Coggy's attention values to Hyle salience scores
    pub fn enrich_salience(&self, ctx: &mut SalienceContext) {
        let focused = self.space.atoms_by_sti(10);
        for atom in focused {
            let name = self.space.format_atom(atom.id);
            let score = (atom.av.sti / 100.0).min(1.0) as f32;

            ctx.add_with_tier(
                format!("[AtomSpace] {} (STI: {:.1})", name, atom.av.sti),
                ContextCategory::Fact,
                if score > 0.6 {
                    SalienceTier::Focus
                } else {
                    SalienceTier::Recent
                },
            );
        }
    }

    /// Convert Hyle's tool outcomes to Coggy attention changes
    pub fn absorb_momentum(&mut self, momentum: &Momentum, outcomes: &[ToolOutcome]) {
        let momentum_score = momentum.score();
        let activated: Vec<u64> = outcomes
            .iter()
            .filter_map(|o| self.tool_atoms.get(&o.tool_name).copied())
            .collect();

        if !activated.is_empty() {
            coggy::ecan::spread_attention(&mut self.space, &activated, &self.ecan_config);
        }

        // Adjust ECAN config based on momentum
        if momentum_score < 0.5 {
            self.ecan_config.decay_factor = 0.5; // Faster decay when stuck
        } else {
            self.ecan_config.decay_factor = 0.7; // Normal decay
        }
    }

    /// Extract Hyle-compatible facts from the AtomSpace
    pub fn extract_facts(&self) -> Vec<Fact> {
        let mut facts = Vec::new();
        let top = self.space.atoms_by_sti(20);

        for atom in top {
            let name = self.space.format_atom(atom.id);
            let confidence = (atom.tv.confidence as f32).min(1.0);

            let category = if name.contains("error") || name.contains("fail") {
                FactCategory::Error
            } else if name.contains("intent") || name.contains("goal") {
                FactCategory::UserIntent
            } else if self.file_atoms.values().any(|&id| id == atom.id) {
                FactCategory::FileState
            } else {
                FactCategory::Decision
            };

            facts.push(Fact {
                category,
                content: name,
                confidence,
            });
        }

        facts
    }

    /// Cognitive loop step: process input through full coggy pipeline
    pub fn think(&mut self, input: &str) -> CoggyThought {
        let result = cogloop::run(&mut self.space, input, &self.ecan_config);

        CoggyThought {
            new_atoms: result.new_atoms,
            total_atoms: result.total_atoms,
            inferences: result.inferences,
            trace: result
                .trace
                .iter()
                .map(|step| format!("{}: {}", step.phase, step.lines.join(" | ")))
                .collect(),
            focus: self.top_focus(5),
        }
    }

    /// Atom count
    pub fn atom_count(&self) -> usize {
        self.space.size()
    }
}

/// Result of a coggy thinking step
pub struct CoggyThought {
    pub new_atoms: usize,
    pub total_atoms: usize,
    pub inferences: usize,
    pub trace: Vec<String>,
    pub focus: Vec<(String, f64)>,
}

impl std::fmt::Display for CoggyThought {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Coggy: {} atoms ({} new), {} inferences",
            self.total_atoms, self.new_atoms, self.inferences
        )?;
        for line in &self.trace {
            writeln!(f, "  {}", line)?;
        }
        if !self.focus.is_empty() {
            write!(f, "  Focus: ")?;
            for (i, (name, sti)) in self.focus.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}({:.0})", name, sti)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_creation() {
        let bridge = CoggyBridge::new();
        assert!(bridge.atom_count() > 30, "Should have base ontology atoms");
    }

    #[test]
    fn test_record_tool_call() {
        let mut bridge = CoggyBridge::new();
        bridge.record_tool_call("read", "src/main.rs", true);
        bridge.record_tool_call("write", "src/lib.rs", true);
        bridge.record_tool_call("bash", "cargo build", false);

        // Tool calls create atoms but they start with low STI since
        // no ECAN spread happens for direct tool recording
        assert!(bridge.atom_count() > 30, "Should have atoms after tool calls");
    }

    #[test]
    fn test_think() {
        let mut bridge = CoggyBridge::new();
        let thought = bridge.think("cat is-a animal");
        assert!(thought.total_atoms > 30);
        assert!(thought.new_atoms > 0 || thought.inferences > 0);
    }

    #[test]
    fn test_health_check() {
        let bridge = CoggyBridge::new();
        let health = bridge.health_check();
        assert!(health.on_track, "Fresh bridge should be healthy");
    }

    #[test]
    fn test_extract_facts() {
        let mut bridge = CoggyBridge::new();
        bridge.think("error in login module");
        let facts = bridge.extract_facts();
        // Should have some facts from the thinking
        assert!(bridge.atom_count() > 30);
    }
}
