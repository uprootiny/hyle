# Dendral Architecture Vision

*A synthesis of formal methods, LLM architecture, software engineering, systems thinking, and distributed systems - reconciled into a coherent approach for cyberhorticulture.*

## The Core Tension

These domains conflict in fundamental ways:

| Domain | Wants | LLMs Provide |
|--------|-------|--------------|
| Formal Methods | Determinism, proofs | Stochastic outputs |
| Software Eng | Pragmatic solutions | Emergent behavior |
| Systems Thinking | Feedback loops | Black-box reasoning |
| Distributed Systems | Consensus | Single-point generation |

**The reconciliation**: We cannot have mathematical guarantees with LLMs. We CAN have empirical reliability through redundancy, checking, and rollback. Contracts aren't proofs—they're checkpoints and guardrails.

## Expert Perspectives

### 1. Formal Methods View

The formal methods expert sees:

```
SPECIFICATION → REFINEMENT → IMPLEMENTATION → VERIFICATION
     ↑                                              │
     └──────────────── feedback ───────────────────┘
```

**What we take**: Pre/post conditions, invariants, refinement hierarchy.
**What we adapt**: Instead of proofs, we use empirical checks. Instead of guaranteed termination, we use bounded iteration with progress metrics.

**Key insight**: The `Contract` type encodes what we WANT to be true. The system's job is to make it true or fail explicitly.

### 2. LLM Architecture View

The LLM expert sees attention as a soft pointer mechanism:

```
Context Window
┌─────────────────────────────────────────────┐
│ [System] [History...] [Current] [Tools]     │
│     ↑         ↑          ↑         ↑        │
│   static   salience   focus    available    │
│   weight   weighted   max       reference   │
└─────────────────────────────────────────────┘
```

**What we take**: Salience-based context management, tiered memory.
**What we adapt**: Make the tiers explicit and controllable. Let contracts influence salience.

**Key insight**: The unfold/fold pattern is about MANAGING ATTENTION across time and abstraction levels.

### 3. Software Engineering View

The practitioner sees reality:

```
IDEAL: Spec → Code → Test → Ship
ACTUAL: Hack → Debug → Patch → Pray → Refactor
```

**What we take**: Incremental progress, test-driven confidence, rollback capability.
**What we adapt**: Make "good enough" explicit. Define when something is DONE vs PERFECT.

**Key insight**: The system should help developers stay in the "good enough" zone, not chase perfection.

### 4. Systems Thinking View

The cyberneticist sees feedback loops everywhere:

```
           ┌─────────────┐
           │   INTENT    │
           └──────┬──────┘
                  │ unfold
                  ▼
           ┌─────────────┐
     ┌────▶│   ACTION    │────┐
     │     └─────────────┘    │
     │                        │
  observe                   effect
     │                        │
     │     ┌─────────────┐    │
     └─────│   WORLD     │◀───┘
           └──────┬──────┘
                  │ fold
                  ▼
           ┌─────────────┐
           │  LEARNING   │
           └─────────────┘
```

**What we take**: The observer/actor/world triad. Feedback as the fundamental mechanism.
**What we adapt**: Make feedback explicit. Track what was observed, what was expected, what was learned.

**Key insight**: Cyberhorticulture is CULTIVATION, not construction. We create conditions for growth, not blueprints for assembly.

### 5. Distributed Systems View

The distributed systems engineer expects failure:

```
ASSUME: Networks partition. Processes crash. Clocks drift.
DESIGN: For graceful degradation, not perfect operation.
```

**What we take**: Idempotency, checkpointing, eventual consistency.
**What we adapt**: LLM responses aren't transactions, but we CAN checkpoint state and retry.

**Key insight**: The cascade of contexts across layers IS a distributed system. Each layer can fail independently.

## The Reconciled Architecture

### Layer Model

```
┌─────────────────────────────────────────────────────────────┐
│ LAYER 5: ECOSYSTEM                                          │
│ - Multi-project coordination                                │
│ - Pattern libraries, conventions                            │
│ - Organizational memory                                     │
│ Contract: Vision-level intents, cross-project invariants    │
├─────────────────────────────────────────────────────────────┤
│ LAYER 4: PROJECT                                            │
│ - Architecture decisions                                    │
│ - Dependency management                                     │
│ - Release coordination                                      │
│ Contract: Mission-level intents, architectural invariants   │
├─────────────────────────────────────────────────────────────┤
│ LAYER 3: SESSION                                            │
│ - Task sequences                                            │
│ - Context continuity                                        │
│ - Intent tracking                                           │
│ Contract: Goal-level intents, session invariants            │
├─────────────────────────────────────────────────────────────┤
│ LAYER 2: ITERATION                                          │
│ - Tool execution                                            │
│ - Response parsing                                          │
│ - Stuck detection                                           │
│ Contract: Task-level intents, iteration invariants          │
├─────────────────────────────────────────────────────────────┤
│ LAYER 1: TOKEN                                              │
│ - Streaming                                                 │
│ - Context window management                                 │
│ - Salience scoring                                          │
│ Contract: Atomic intents, token-level checks                │
└─────────────────────────────────────────────────────────────┘
```

### The Unfold/Fold Cascade

```
UNFOLD (expansion):
  Layer N receives intent
  → Checks preconditions
  → Decomposes into sub-intents (Layer N-1)
  → Monitors invariants
  → Aggregates results

FOLD (compression):
  Layer N-1 completes
  → Summarizes results
  → Updates beliefs/models
  → Reports to Layer N
  → Layer N checks postconditions
```

### Information Flow

```
         UNFOLD ───────────────────────▶

Layer 5  ═══╦══════════════════════════════════╦═══
             ║  spawn project                   ║ patterns
Layer 4  ═══╬══════════════════════════════════╬═══
             ║  task breakdown                  ║ decisions
Layer 3  ═══╬══════════════════════════════════╬═══
             ║  tool calls                      ║ summaries
Layer 2  ═══╬══════════════════════════════════╬═══
             ║  tokens                          ║ parsed
Layer 1  ═══╩══════════════════════════════════╩═══

         ◀─────────────────────────── FOLD
```

## Implementation Roadmap

### Phase 1: Foundation (Current - v0.3.x)

**Goal**: Reliable single-layer operation

- [x] Atomic file writes with verification
- [x] Contract types (Intent, Invariant, Obligation, Criterion)
- [x] Agent autonomy improvements
- [x] UX metrics framework
- [ ] Contract enforcement in tool execution
- [ ] Checkpoint/rollback for file operations
- [ ] Basic invariant checking (tests stay green)

### Phase 2: Layer 2-3 Integration (v0.4.x)

**Goal**: Session-aware iteration management

- [ ] Intent stack across iterations
- [ ] Automatic folding (summarization) of completed tasks
- [ ] Unfold triggers (when to decompose a task)
- [ ] Cross-iteration learning (what worked, what didn't)
- [ ] Progress-based contract satisfaction

### Phase 3: Layer 3-4 Bridge (v0.5.x)

**Goal**: Project-level continuity

- [ ] Persistent project state (architecture decisions, conventions)
- [ ] Session handoff with contract preservation
- [ ] Multi-session goals (missions)
- [ ] Project-level invariants (don't break the build across sessions)

### Phase 4: Layer 4-5 Emergence (v0.6.x)

**Goal**: Cross-project coordination

- [ ] Pattern extraction from successful projects
- [ ] Convention libraries
- [ ] Multi-project orchestration
- [ ] Organizational memory

### Phase 5: Cyberhorticulture (v1.0)

**Goal**: Self-sustaining software cultivation

- [ ] Autonomous background improvement
- [ ] Proactive issue detection
- [ ] Self-bootstrapping (hyle improves hyle)
- [ ] Ecosystem-level health monitoring

## Key Design Decisions

### 1. Contracts are Empirical, Not Proofs

We cannot prove an LLM will produce correct code. We CAN:
- Check outputs against criteria
- Rollback when invariants are violated
- Retry with different approaches
- Track success rates and learn

### 2. Layers are Semi-Permeable

Information flows UP through folding (summarization).
Information flows DOWN through unfolding (decomposition).
But layers can also SKIP when needed:
- Layer 3 can directly invoke Layer 1 for simple tasks
- Layer 5 patterns can directly constrain Layer 2 tool calls

### 3. Failure is Information

When a contract fails:
- The failure mode is recorded
- Alternative approaches are tried
- Patterns of failure inform future attempts
- Some failures are terminal, others are learning opportunities

### 4. "Done" is Context-Dependent

What counts as done depends on the intent level:
- Atomic: Tool executed without error
- Task: Criteria satisfied, tests pass
- Goal: User-defined acceptance
- Mission: Multiple goals achieved
- Vision: Ongoing cultivation, never truly "done"

## Metrics for Success

### Layer 1-2: Technical Metrics
- First-token latency (p50, p95)
- Tool success rate
- Context utilization efficiency

### Layer 2-3: Task Metrics
- Autonomous completion rate
- Average iterations to completion
- Stuck detection accuracy

### Layer 3-4: Session Metrics
- Intent completion rate
- Rollback frequency
- User intervention rate

### Layer 4-5: Project Metrics
- Architecture coherence (no contradictory decisions)
- Convention consistency
- Cross-session knowledge retention

### Ecosystem Metrics
- Pattern reuse rate
- Time to productive new project
- Self-improvement rate

## The Cyberhorticulture Metaphor

Traditional software development: **Construction**
- Blueprint → Materials → Assembly → Inspection
- Deterministic, planned, controlled

Cyberhorticulture: **Cultivation**
- Seed → Soil → Growth → Pruning → Harvest → Compost → Seed
- Stochastic, emergent, guided

The gardener doesn't BUILD a tomato. They create CONDITIONS for tomatoes to grow:
- Good soil (solid contracts)
- Proper watering (regular feedback)
- Pruning (removing failed branches)
- Trellising (guiding growth patterns)
- Patience (allowing emergence)

hyle should be a GARDENING TOOL, not a CONSTRUCTION MACHINE.

---

*"The best time to plant a tree was 20 years ago. The second best time is now."*

*The best time to formalize your software contracts was at the start. The second best time is now.*
