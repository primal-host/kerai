# Plan 09: Agent Swarms

*Depends on: Plan 04 (CRDT Operations), Plan 06 (Distribution), Plan 08 (AI Perspectives)*
*Enables: —*

## Goal

Scale kerai to support massive concurrent agent workloads — hundreds, thousands, or a million agents working on the same codebase simultaneously, converging toward test-passing solutions. This plan addresses the infrastructure, coordination patterns, and observability needed to make swarm-scale development practical.

## The Development Model Shift

Traditional: a developer writes code, runs tests, iterates.

Swarm: you describe the problem, define the tests, unleash agents, and the codebase evolves *toward passing tests* as a convergent process. The database stores an evolving population of solutions. Agents read the current best state, propose mutations as CRDT operations on AST nodes, and the mutations that move tests from red to green are retained.

This is closer to evolutionary search than traditional development.

## Deliverables

### 9.1 Agent Lifecycle Management

```bash
# Register an agent
kerai agent register --name solver-1 --kind llm --model claude-opus-4-6

# Launch a swarm of agents against a task
kerai swarm launch \
  --task "make all tests in pkg/auth pass" \
  --agents 100 \
  --model claude-opus-4-6 \
  --timeout 1h

# Monitor running agents
kerai swarm status

# Stop a swarm
kerai swarm stop <swarm-id>
```

### 9.2 Task Definition

A task is a node in the database (everything is a node) with:

- A description (natural language)
- A success criterion (test commands, assertions, or structural checks)
- A scope (which package/subtree agents should focus on)
- A budget (max operations, max time, max cost)

```sql
CREATE TABLE tasks (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    description     text NOT NULL,
    scope_node_id   uuid REFERENCES nodes(id),  -- subtree agents work on
    success_command text NOT NULL,                -- e.g., "cargo test --package auth"
    budget_ops      integer,                      -- max operations per agent
    budget_seconds  integer,                      -- max wall-clock time
    status          text DEFAULT 'pending',       -- pending, running, succeeded, failed
    created_at      timestamptz DEFAULT now()
);
```

### 9.3 Branching via Version Vectors

Agents don't need git-style branches. A branch is just a version vector — a snapshot of state that an agent starts from:

```
-- Agent starts from current state
base_vector = {billy: 147, agent-1: 83}

-- Agent makes 50 operations
agent_vector = {billy: 147, agent-1: 83, solver-42: 50}

-- If tests pass, merge by broadcasting ops to the main database
-- If tests fail, discard — the ops were never pushed
```

No branch creation, no merge ceremony, no cleanup. An agent that fails simply doesn't share its operations. An agent that succeeds pushes its ops, and CRDT convergence handles the rest.

### 9.4 Test-Driven Convergence

The swarm coordination loop:

1. **Read:** Agent takes a snapshot of the current state (MVCC gives a consistent view)
2. **Mutate:** Agent modifies AST nodes within its scope
3. **Test:** Agent reconstructs source (Plan 03), runs the test command
4. **Evaluate:**
   - Tests pass → push operations to the shared database
   - Tests fail → discard operations (or push to a "failed attempts" log for learning)
5. **Repeat:** Agent reads the new state (which may include other agents' successful changes) and tries again

```sql
-- Record test results
CREATE TABLE test_results (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id         uuid NOT NULL REFERENCES tasks(id),
    agent_id        uuid NOT NULL REFERENCES agents(id),
    version_vector  jsonb NOT NULL,       -- state that was tested
    passed          boolean NOT NULL,
    output          text,                 -- test output for debugging
    duration_ms     integer,
    created_at      timestamptz DEFAULT now()
);

CREATE INDEX idx_test_results_task ON test_results(task_id);
CREATE INDEX idx_test_results_passed ON test_results(passed);
CREATE INDEX idx_test_results_agent ON test_results(agent_id);
```

### 9.5 Swarm Observability

Real-time dashboards and queries for monitoring swarm behavior:

```bash
# Which agents are producing passing changes?
kerai swarm leaderboard --task <task-id>

# What parts of the codebase are being modified most?
kerai swarm hotspots --since 1h

# Show the evolution of test pass rate over time
kerai swarm progress --task <task-id>

# Show an agent's operation history
kerai swarm trace --agent solver-42
```

Underlying queries:

```sql
-- Agent effectiveness
SELECT a.name,
    count(*) FILTER (WHERE tr.passed) as passes,
    count(*) FILTER (WHERE NOT tr.passed) as failures,
    round(100.0 * count(*) FILTER (WHERE tr.passed) / count(*), 1) as pass_rate
FROM test_results tr
JOIN agents a ON tr.agent_id = a.id
WHERE tr.task_id = '<task-id>'
GROUP BY a.name
ORDER BY pass_rate DESC;

-- Convergence rate: how fast are tests going green?
SELECT
    date_trunc('minute', created_at) as minute,
    count(*) FILTER (WHERE passed) as passes,
    count(*) FILTER (WHERE NOT passed) as failures
FROM test_results
WHERE task_id = '<task-id>'
GROUP BY minute
ORDER BY minute;

-- Lineage of a function from failing to passing
SELECT o.*, tr.passed
FROM operations o
JOIN nodes n ON o.node_id = n.id
LEFT JOIN test_results tr ON tr.version_vector @> jsonb_build_object(o.author, o.author_seq)
WHERE n.path <@ 'crate.auth.validate_token'
ORDER BY o.lamport_ts;
```

### 9.6 Postgres Scaling for Swarm Workloads

At swarm scale, the database sees:
- High write throughput on `operations` (many agents committing concurrently)
- High read throughput on `nodes` (agents reading current state)
- Moderate write throughput on `test_results`

Postgres tuning for this workload:

```sql
-- Partition operations by author for write throughput
CREATE TABLE operations (
    ...
) PARTITION BY HASH (author);

-- Create partitions (one per expected concurrent agent group)
CREATE TABLE operations_p0 PARTITION OF operations FOR VALUES WITH (MODULUS 16, REMAINDER 0);
CREATE TABLE operations_p1 PARTITION OF operations FOR VALUES WITH (MODULUS 16, REMAINDER 1);
-- ... through p15

-- Unlogged tables for test_results (ephemeral, can be regenerated)
-- Faster writes at the cost of crash recovery
CREATE UNLOGGED TABLE test_results (...);
```

Connection pooling via PgBouncer or Postgres's built-in connection limits:
- 1,000 agents with connection pooling: ~50-100 actual Postgres connections
- 1,000,000 agents: tiered architecture with agent groups sharing connections

### 9.7 Version Vector Compression

At swarm scale, version vectors with one entry per agent become unwieldy. Solution: hierarchical grouping.

```
-- Instead of a million entries:
{solver-1: 50, solver-2: 48, solver-3: 52, ...}

-- Group by swarm job:
{billy: 147, swarm-job-58a3: 12041}

-- Where swarm-job-58a3's internal state is tracked separately
-- The shared database only sees the aggregate
```

An agent swarm shares a single identity in the version vector. Internally, the swarm tracks per-agent state. When ops are pushed to the shared database, they're attributed to the swarm job, not individual agents.

## Decisions to Make

- **Agent orchestration:** Should kerai itself orchestrate agents (launching LLM API calls), or should it be a passive database that external orchestrators write to? Proposed: start passive — agents are external processes that connect via the standard CLI/SQL. Add built-in orchestration later.
- **Failed attempt retention:** Should failed mutations be stored for analysis? Proposed: store in a separate `failed_operations` table for post-mortem analysis, with automatic TTL-based cleanup.
- **Resource limits:** How to prevent a swarm from overwhelming the database? Proposed: per-task rate limits (max ops/sec), per-agent budgets (max ops per attempt), and circuit breakers that pause the swarm if Postgres load exceeds thresholds.
- **Multi-machine swarms:** A million agents won't run on one machine. The connection string model already supports pointing agents at a remote Postgres. But should kerai also help launch agents across multiple machines? Proposed: out of scope — use existing orchestration tools (Kubernetes, Docker Swarm, etc.) to distribute agents. Kerai just needs to handle the database side.

## Out of Scope

- Agent intelligence / prompting strategies (how to make agents write good code)
- Cost management (tracking API spend across LLM providers)
- Agent communication beyond the shared database (agents don't talk to each other — they communicate through the codebase)
