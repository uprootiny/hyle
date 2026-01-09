//! Contracts - Formal specification of intents, invariants, and obligations
//!
//! This module provides the foundation for rigorous task management:
//! - Intents: What we're trying to accomplish
//! - Invariants: What must always be true
//! - Obligations: What must happen before proceeding
//! - Completion: What counts as done
//! - Failure modes: What counts as failed, recovery options
//!
//! These contracts enable the unfold/fold cascade to operate correctly
//! by making implicit assumptions explicit and checkable.

use std::collections::HashSet;
use std::fmt;

// ═══════════════════════════════════════════════════════════════
// INTENT - What are we trying to accomplish?
// ═══════════════════════════════════════════════════════════════

/// An intent at any granularity level
#[derive(Debug, Clone)]
pub struct Intent {
    /// Unique identifier
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// Granularity level (higher = more abstract)
    pub level: IntentLevel,
    /// Parent intent (if this is a sub-intent)
    pub parent: Option<String>,
    /// Completion criteria - ALL must be satisfied
    pub done_when: Vec<Criterion>,
    /// Failure criteria - ANY triggers failure
    pub failed_when: Vec<Criterion>,
    /// Invariants that must hold throughout
    pub invariants: Vec<Invariant>,
    /// Obligations before we can start
    pub preconditions: Vec<Obligation>,
    /// Obligations after completion
    pub postconditions: Vec<Obligation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntentLevel {
    /// Single tool call
    Atomic = 0,
    /// Multiple tool calls, single purpose
    Task = 1,
    /// Multiple tasks, coherent goal
    Goal = 2,
    /// Multiple goals, session-spanning
    Mission = 3,
    /// Multiple missions, project-spanning
    Vision = 4,
}

impl fmt::Display for IntentLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntentLevel::Atomic => write!(f, "atomic"),
            IntentLevel::Task => write!(f, "task"),
            IntentLevel::Goal => write!(f, "goal"),
            IntentLevel::Mission => write!(f, "mission"),
            IntentLevel::Vision => write!(f, "vision"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// CRITERION - What counts as done/failed?
// ═══════════════════════════════════════════════════════════════

/// A criterion for completion or failure
#[derive(Debug, Clone)]
pub struct Criterion {
    /// Human-readable description
    pub description: String,
    /// The check to perform
    pub check: Check,
    /// Is this criterion currently satisfied?
    pub satisfied: bool,
}

/// Types of checks we can perform
#[derive(Debug, Clone)]
pub enum Check {
    /// A file exists at the given path
    FileExists(String),
    /// A file contains the given pattern
    FileContains { path: String, pattern: String },
    /// A command exits with code 0
    CommandSucceeds(String),
    /// A command's output contains pattern
    CommandOutputContains { command: String, pattern: String },
    /// Tests pass
    TestsPass,
    /// Build succeeds
    BuildSucceeds,
    /// No compiler warnings
    NoWarnings,
    /// Custom predicate (description only, checked externally)
    Custom(String),
    /// All sub-intents are complete
    SubIntentsComplete,
    /// User confirms
    UserConfirms(String),
}

impl Criterion {
    pub fn file_exists(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            description: format!("File exists: {}", path),
            check: Check::FileExists(path),
            satisfied: false,
        }
    }

    pub fn tests_pass() -> Self {
        Self {
            description: "All tests pass".into(),
            check: Check::TestsPass,
            satisfied: false,
        }
    }

    pub fn build_succeeds() -> Self {
        Self {
            description: "Build succeeds".into(),
            check: Check::BuildSucceeds,
            satisfied: false,
        }
    }

    pub fn custom(description: impl Into<String>) -> Self {
        let desc = description.into();
        Self {
            description: desc.clone(),
            check: Check::Custom(desc),
            satisfied: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// INVARIANT - What must always be true?
// ═══════════════════════════════════════════════════════════════

/// An invariant that must hold throughout execution
#[derive(Debug, Clone)]
pub struct Invariant {
    /// Human-readable description
    pub description: String,
    /// The condition to maintain
    pub condition: InvariantCondition,
    /// What to do if violated
    pub on_violation: ViolationResponse,
    /// Has this been violated?
    pub violated: bool,
}

#[derive(Debug, Clone)]
pub enum InvariantCondition {
    /// File must not be modified
    FileUnchanged(String),
    /// File must not be deleted
    FileExists(String),
    /// Directory must not contain more than N files
    MaxFiles { path: String, max: usize },
    /// Process must not exceed memory limit
    MemoryLimit(usize),
    /// No destructive commands
    NoDestructiveCommands,
    /// Tests must continue passing
    TestsStayGreen,
    /// Custom (checked externally)
    Custom(String),
}

#[derive(Debug, Clone)]
pub enum ViolationResponse {
    /// Stop immediately, no recovery
    Halt,
    /// Attempt to restore from backup
    Rollback,
    /// Warn but continue
    Warn,
    /// Trigger a repair action
    Repair(String),
}

impl Invariant {
    pub fn no_destructive_commands() -> Self {
        Self {
            description: "No destructive commands (rm -rf, etc)".into(),
            condition: InvariantCondition::NoDestructiveCommands,
            on_violation: ViolationResponse::Halt,
            violated: false,
        }
    }

    pub fn tests_stay_green() -> Self {
        Self {
            description: "Tests must continue passing".into(),
            condition: InvariantCondition::TestsStayGreen,
            on_violation: ViolationResponse::Rollback,
            violated: false,
        }
    }

    pub fn file_unchanged(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            description: format!("File unchanged: {}", path),
            condition: InvariantCondition::FileUnchanged(path),
            on_violation: ViolationResponse::Rollback,
            violated: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// OBLIGATION - What must happen?
// ═══════════════════════════════════════════════════════════════

/// An obligation that must be fulfilled
#[derive(Debug, Clone)]
pub struct Obligation {
    /// Human-readable description
    pub description: String,
    /// The required action/state
    pub requirement: Requirement,
    /// Has this been fulfilled?
    pub fulfilled: bool,
    /// When was it fulfilled?
    pub fulfilled_at: Option<std::time::Instant>,
}

#[derive(Debug, Clone)]
pub enum Requirement {
    /// Must read file before modifying
    ReadBeforeWrite(String),
    /// Must run tests after changes
    TestAfterChange,
    /// Must create backup before destructive action
    BackupBefore(String),
    /// Must get user confirmation
    UserConfirmation(String),
    /// Must document the change
    DocumentChange,
    /// Must commit with message
    CommitWithMessage,
    /// Custom requirement
    Custom(String),
}

impl Obligation {
    pub fn read_before_write(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            description: format!("Read {} before modifying", path),
            requirement: Requirement::ReadBeforeWrite(path),
            fulfilled: false,
            fulfilled_at: None,
        }
    }

    pub fn test_after_change() -> Self {
        Self {
            description: "Run tests after making changes".into(),
            requirement: Requirement::TestAfterChange,
            fulfilled: false,
            fulfilled_at: None,
        }
    }

    pub fn backup_before(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            description: format!("Backup {} before changes", path),
            requirement: Requirement::BackupBefore(path),
            fulfilled: false,
            fulfilled_at: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// CONTRACT - The complete specification
// ═══════════════════════════════════════════════════════════════

/// A complete contract for a unit of work
#[derive(Debug, Clone)]
pub struct Contract {
    /// The intent this contract governs
    pub intent: Intent,
    /// Current state
    pub state: ContractState,
    /// History of state transitions
    pub transitions: Vec<StateTransition>,
    /// Files touched (for rollback)
    pub touched_files: HashSet<String>,
    /// Checkpoints for rollback
    pub checkpoints: Vec<Checkpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractState {
    /// Not yet started
    Pending,
    /// Preconditions being checked
    CheckingPreconditions,
    /// Actively working
    InProgress,
    /// Checking completion criteria
    CheckingCompletion,
    /// Successfully completed
    Complete,
    /// Failed, may be recoverable
    Failed,
    /// Rolled back to previous state
    RolledBack,
    /// Halted due to invariant violation
    Halted,
}

#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: ContractState,
    pub to: ContractState,
    pub reason: String,
    pub timestamp: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub id: String,
    pub description: String,
    pub timestamp: std::time::Instant,
    /// Snapshot of file contents at this point
    pub file_snapshots: Vec<(String, Vec<u8>)>,
}

impl Contract {
    /// Create a new contract for an intent
    pub fn new(intent: Intent) -> Self {
        Self {
            intent,
            state: ContractState::Pending,
            transitions: vec![],
            touched_files: HashSet::new(),
            checkpoints: vec![],
        }
    }

    /// Transition to a new state
    pub fn transition(&mut self, to: ContractState, reason: impl Into<String>) {
        let from = self.state;
        self.transitions.push(StateTransition {
            from,
            to,
            reason: reason.into(),
            timestamp: std::time::Instant::now(),
        });
        self.state = to;
    }

    /// Check if all preconditions are met
    pub fn preconditions_met(&self) -> bool {
        self.intent.preconditions.iter().all(|o| o.fulfilled)
    }

    /// Check if all completion criteria are satisfied
    pub fn is_complete(&self) -> bool {
        self.intent.done_when.iter().all(|c| c.satisfied)
    }

    /// Check if any failure criteria are met
    pub fn is_failed(&self) -> bool {
        self.intent.failed_when.iter().any(|c| c.satisfied)
    }

    /// Check if any invariants are violated
    pub fn invariants_ok(&self) -> bool {
        self.intent.invariants.iter().all(|i| !i.violated)
    }

    /// Record a file touch for potential rollback
    pub fn touch_file(&mut self, path: impl Into<String>) {
        self.touched_files.insert(path.into());
    }

    /// Create a checkpoint
    pub fn checkpoint(&mut self, description: impl Into<String>) {
        let id = format!("cp_{}", self.checkpoints.len());
        self.checkpoints.push(Checkpoint {
            id,
            description: description.into(),
            timestamp: std::time::Instant::now(),
            file_snapshots: vec![], // Would be populated by caller
        });
    }

    /// Summary for display
    pub fn summary(&self) -> String {
        let done_count = self.intent.done_when.iter().filter(|c| c.satisfied).count();
        let done_total = self.intent.done_when.len();
        let invariant_ok = self.invariants_ok();

        format!(
            "[{}] {} ({:?}) - {}/{} criteria, invariants: {}",
            self.intent.level,
            self.intent.description,
            self.state,
            done_count,
            done_total,
            if invariant_ok { "OK" } else { "VIOLATED" }
        )
    }
}

// ═══════════════════════════════════════════════════════════════
// CONTRACT BUILDER - Fluent API for creating contracts
// ═══════════════════════════════════════════════════════════════

/// Builder for creating contracts
pub struct ContractBuilder {
    id: String,
    description: String,
    level: IntentLevel,
    parent: Option<String>,
    done_when: Vec<Criterion>,
    failed_when: Vec<Criterion>,
    invariants: Vec<Invariant>,
    preconditions: Vec<Obligation>,
    postconditions: Vec<Obligation>,
}

impl ContractBuilder {
    pub fn new(description: impl Into<String>) -> Self {
        let desc = description.into();
        Self {
            id: uuid_simple(),
            description: desc,
            level: IntentLevel::Task,
            parent: None,
            done_when: vec![],
            failed_when: vec![],
            invariants: vec![Invariant::no_destructive_commands()], // Default safety
            preconditions: vec![],
            postconditions: vec![],
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn level(mut self, level: IntentLevel) -> Self {
        self.level = level;
        self
    }

    pub fn parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    pub fn done_when(mut self, criterion: Criterion) -> Self {
        self.done_when.push(criterion);
        self
    }

    pub fn failed_when(mut self, criterion: Criterion) -> Self {
        self.failed_when.push(criterion);
        self
    }

    pub fn invariant(mut self, invariant: Invariant) -> Self {
        self.invariants.push(invariant);
        self
    }

    pub fn precondition(mut self, obligation: Obligation) -> Self {
        self.preconditions.push(obligation);
        self
    }

    pub fn postcondition(mut self, obligation: Obligation) -> Self {
        self.postconditions.push(obligation);
        self
    }

    /// Must have tests pass before done
    pub fn requires_tests(self) -> Self {
        self.done_when(Criterion::tests_pass())
            .postcondition(Obligation::test_after_change())
    }

    /// Must have build succeed before done
    pub fn requires_build(self) -> Self {
        self.done_when(Criterion::build_succeeds())
    }

    /// Must keep tests green throughout
    pub fn tests_must_stay_green(self) -> Self {
        self.invariant(Invariant::tests_stay_green())
    }

    pub fn build(self) -> Contract {
        let intent = Intent {
            id: self.id,
            description: self.description,
            level: self.level,
            parent: self.parent,
            done_when: self.done_when,
            failed_when: self.failed_when,
            invariants: self.invariants,
            preconditions: self.preconditions,
            postconditions: self.postconditions,
        };
        Contract::new(intent)
    }
}

/// Simple UUID-like ID generator
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", nanos)
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_builder() {
        let contract = ContractBuilder::new("Add login feature")
            .level(IntentLevel::Goal)
            .done_when(Criterion::file_exists("src/auth.rs"))
            .done_when(Criterion::tests_pass())
            .requires_build()
            .tests_must_stay_green()
            .build();

        assert_eq!(contract.state, ContractState::Pending);
        assert_eq!(contract.intent.level, IntentLevel::Goal);
        assert_eq!(contract.intent.done_when.len(), 3); // file + tests + build
        assert!(!contract.intent.invariants.is_empty()); // has default + tests_stay_green
    }

    #[test]
    fn test_contract_state_transitions() {
        let mut contract = ContractBuilder::new("Simple task")
            .done_when(Criterion::custom("Something done"))
            .build();

        assert_eq!(contract.state, ContractState::Pending);

        contract.transition(ContractState::InProgress, "Starting work");
        assert_eq!(contract.state, ContractState::InProgress);
        assert_eq!(contract.transitions.len(), 1);

        contract.transition(ContractState::Complete, "All done");
        assert_eq!(contract.state, ContractState::Complete);
        assert_eq!(contract.transitions.len(), 2);
    }

    #[test]
    fn test_completion_check() {
        let mut contract = ContractBuilder::new("Task with criteria")
            .done_when(Criterion::custom("Step 1"))
            .done_when(Criterion::custom("Step 2"))
            .build();

        assert!(!contract.is_complete());

        contract.intent.done_when[0].satisfied = true;
        assert!(!contract.is_complete()); // Still need step 2

        contract.intent.done_when[1].satisfied = true;
        assert!(contract.is_complete()); // Now complete
    }

    #[test]
    fn test_invariant_violation() {
        let mut contract = ContractBuilder::new("Protected task")
            .invariant(Invariant::file_unchanged("important.txt"))
            .build();

        assert!(contract.invariants_ok());

        contract.intent.invariants[1].violated = true; // Index 1 because 0 is default no_destructive
        assert!(!contract.invariants_ok());
    }

    #[test]
    fn test_intent_levels() {
        assert!(IntentLevel::Atomic < IntentLevel::Task);
        assert!(IntentLevel::Task < IntentLevel::Goal);
        assert!(IntentLevel::Goal < IntentLevel::Mission);
        assert!(IntentLevel::Mission < IntentLevel::Vision);
    }

    #[test]
    fn test_obligation_tracking() {
        let mut obligation = Obligation::read_before_write("config.rs");
        assert!(!obligation.fulfilled);

        obligation.fulfilled = true;
        obligation.fulfilled_at = Some(std::time::Instant::now());
        assert!(obligation.fulfilled);
    }

    #[test]
    fn test_contract_summary() {
        let contract = ContractBuilder::new("Test task")
            .level(IntentLevel::Task)
            .done_when(Criterion::tests_pass())
            .build();

        let summary = contract.summary();
        assert!(summary.contains("task"));
        assert!(summary.contains("Test task"));
        assert!(summary.contains("Pending"));
    }
}
