# Plan 20: Active Inference — Expected Free Energy for Agent Task Selection

*Depends on: Plan 08 (AI Perspectives), Plan 09 (Agent Swarms), Plan 13 (MicroGPT)*
*Enables: —*

## Source

Thomas Parr, Giovanni Pezzulo, and Karl J. Friston. *Active Inference: The Free Energy Principle in Mind, Brain, and Behavior.* MIT Press, 2022. Open access. Ingested into kerai as 11 markdown documents (2,909 nodes).

## The Core Idea

Active Inference says all adaptive behavior reduces to one imperative: **minimize expected free energy**. An agent selects policies (actions) that jointly reduce uncertainty about the world (epistemic value) and bring the world closer to preferred states (pragmatic value). This is not a metaphor — it is a computable quantity over a generative model.

Kerai's node graph *is* a generative model. The AST hierarchy predicts structure: a `fn` node predicts `param` and `block` children; a `struct` predicts `field` children. Edges encode statistical dependencies. Perspectives encode beliefs. The entire database is already the substrate that Active Inference operates on — what's missing is the scoring function that turns the graph into a task priority queue.

Plan 09 defines agent swarms but leaves task selection to external orchestrators. This plan gives agents an intrinsic selection mechanism: score every candidate action by its expected free energy, pick the action with the lowest G (highest combined information gain and goal fulfillment). No hand-tuned heuristics, no ad hoc priority rules — one principled objective.

## Background: Expected Free Energy

From Chapter 7 of Parr et al., the expected free energy G for a policy π decomposes as:

```
G(π) = ambiguity + risk
     = E_Q[H(o|s,π)]  +  D_KL[Q(o|π) || P(o)]

     = −epistemic_value − pragmatic_value
```

Where:
- **Epistemic value** (negative ambiguity): How much would executing this policy reduce the agent's uncertainty? Policies that resolve uncertainty are preferred.
- **Pragmatic value** (negative risk): How well does the expected outcome match the agent's prior preferences? Policies whose outcomes match goals are preferred.

The agent selects the policy π that minimizes G — equivalently, maximizes the sum of epistemic and pragmatic value.

## Mapping to Kerai

| Active Inference | Kerai |
|---|---|
| States (s) | Node states: kind, content, metadata, edges, spans |
| Observations (o) | What the agent discovers upon examining/modifying a node |
| Policies (π) | Candidate tasks: examine node X, fix function Y, document module Z |
| Generative model | The node/edge graph with its hierarchical structure |
| Prior preferences P(o) | Open tasks, bounties, test assertions, coverage targets |
| Beliefs Q(s) | Agent's perspectives (Plan 08) — weighted view of the graph |
| Precision | Koi value of a node or perspective — market-determined confidence |

## Deliverables

### 20.1 Node Entropy — The Epistemic Signal

A node's entropy measures how much an agent would learn by examining it. High entropy = high epistemic value = the agent should look here.

Node entropy is computed from the node's structural properties — the absence of information is the signal:

```sql
-- Node entropy scoring function
CREATE OR REPLACE FUNCTION kerai.node_entropy(target_id uuid)
RETURNS float AS $$
    SELECT
        -- Edge sparsity: fewer connections = more unknown
        COALESCE(1.0 / NULLIF(ln(1 + edge_count), 0), 1.0) * 0.3

        -- Perspective sparsity: fewer perspectives = less understood
        + COALESCE(1.0 / NULLIF(ln(1 + perspective_count), 0), 1.0) * 0.25

        -- Documentation gap: undocumented nodes are uncertain
        + CASE WHEN has_doc THEN 0.0 ELSE 1.0 END * 0.2

        -- Metadata sparsity: less metadata = less known
        + COALESCE(1.0 / NULLIF(ln(1 + metadata_keys), 0), 1.0) * 0.1

        -- Staleness: old nodes with no recent edges are uncertain
        + EXTRACT(EPOCH FROM now() - last_touched) / 86400.0 * 0.01

        -- Suggestion load: unresolved suggestions = known unknowns
        + suggestion_count * 0.15
    FROM (
        SELECT
            (SELECT count(*) FROM kerai.edges
             WHERE source_id = target_id OR target_id = target_id) as edge_count,
            (SELECT count(*) FROM kerai.perspectives
             WHERE node_id = target_id) as perspective_count,
            EXISTS(SELECT 1 FROM kerai.edges e
                   JOIN kerai.nodes doc ON e.source_id = doc.id
                   WHERE e.target_id = target_id
                   AND e.relation = 'documents') as has_doc,
            (SELECT count(*) FROM jsonb_object_keys(
                (SELECT metadata FROM kerai.nodes WHERE id = target_id)
            )) as metadata_keys,
            (SELECT COALESCE(max(created_at), '2020-01-01')
             FROM kerai.edges WHERE source_id = target_id) as last_touched,
            (SELECT count(*) FROM kerai.nodes s
             JOIN kerai.edges e ON e.source_id = s.id
             WHERE e.target_id = target_id
             AND e.relation = 'suggests'
             AND s.metadata->>'status' = 'emitted') as suggestion_count
    ) stats;
$$ LANGUAGE sql STABLE;
```

This is computable over the existing schema with no new tables. The weights (0.3, 0.25, etc.) are initial values — Plan 13's MicroGPT can learn better weights from agent behavior.

### 20.2 Pragmatic Value — The Goal Signal

Pragmatic value scores how well examining or modifying a node serves the agent's current goals. Goals come from three sources:

**a) Open tasks (Plan 09):**

A node's pragmatic value increases when it falls within the scope of an open task with budget:

```sql
CREATE OR REPLACE FUNCTION kerai.node_pragmatic_value(
    target_id uuid,
    agent_id uuid DEFAULT NULL
) RETURNS float AS $$
    SELECT COALESCE(task_value, 0) + COALESCE(bounty_value, 0) + COALESCE(consensus_value, 0)
    FROM (
        SELECT
            -- Task alignment: is this node in scope of an active task?
            (SELECT sum(t.budget_nkoi::float / 1e9)
             FROM kerai.tasks t
             WHERE t.status IN ('pending', 'running')
             AND (
                 t.scope_node_id = target_id
                 OR t.scope_node_id IN (
                     SELECT parent_id FROM kerai.nodes WHERE id = target_id
                 )
             )
            ) as task_value,

            -- Bounty proximity: are there bounties referencing this area?
            (SELECT sum(b.reward_nkoi::float / 1e9)
             FROM kerai.bounties b
             WHERE b.status = 'open'
             AND b.scope_node_id IN (
                 -- Ancestors of target
                 WITH RECURSIVE ancestors AS (
                     SELECT id, parent_id FROM kerai.nodes WHERE id = target_id
                     UNION ALL
                     SELECT n.id, n.parent_id FROM kerai.nodes n
                     JOIN ancestors a ON n.id = a.parent_id
                 )
                 SELECT id FROM ancestors
             )
            ) as bounty_value,

            -- Consensus signal: do other agents think this is important?
            (SELECT avg(p.weight)
             FROM kerai.perspectives p
             WHERE p.node_id = target_id
             AND (agent_id IS NULL OR p.agent_id != agent_id)
             AND p.weight > 0.5
            ) as consensus_value
    ) scores;
$$ LANGUAGE sql STABLE;
```

**b) Test failures:** When a test fails, the scope nodes identified by the task's `success_command` become high pragmatic value targets. The agent that fixes a failing test earns the convergence reward.

**c) Prior preferences:** An agent can declare preferences as perspective weights. A high self-perspective on a node means "I care about this" — pragmatic value aligned to the agent's own goals, not just external tasks.

### 20.3 Expected Free Energy Score

The composite score that agents use to select what to work on:

```sql
CREATE OR REPLACE FUNCTION kerai.expected_free_energy(
    target_id uuid,
    agent_id uuid DEFAULT NULL,
    epistemic_weight float DEFAULT 1.0,
    pragmatic_weight float DEFAULT 1.0
) RETURNS float AS $$
    -- Lower G = better. Negate so that higher scores = more attractive.
    SELECT -(
        epistemic_weight * kerai.node_entropy(target_id)
        + pragmatic_weight * kerai.node_pragmatic_value(target_id, agent_id)
    );
$$ LANGUAGE sql STABLE;
```

The `epistemic_weight` and `pragmatic_weight` parameters let agents tune their exploration-exploitation balance:
- **Curious agent** (epistemic_weight=2.0, pragmatic_weight=0.5): explores unknown territory
- **Focused agent** (epistemic_weight=0.5, pragmatic_weight=2.0): pursues known goals
- **Balanced agent** (1.0, 1.0): the default Active Inference agent

### 20.4 Task Selection Query

The core query an agent runs to decide what to do next:

```sql
-- What should agent X work on?
SELECT
    n.id,
    n.kind,
    n.content,
    kerai.node_entropy(n.id) as epistemic,
    kerai.node_pragmatic_value(n.id, '<agent-id>') as pragmatic,
    kerai.expected_free_energy(n.id, '<agent-id>') as G
FROM kerai.nodes n
WHERE n.kind IN ('fn', 'struct', 'module', 'file', 'document')
ORDER BY G ASC  -- minimize expected free energy
LIMIT 10;
```

This replaces ad-hoc task assignment with a principled priority queue. The agent with the lowest-G node in its scope has the highest-value action available.

### 20.5 Belief Updating via Perspectives

After an agent examines or modifies a node, it updates its beliefs — recording what it learned as perspective weights (Plan 08). This closes the Active Inference loop:

1. **Score** candidate nodes by expected free energy → select the lowest G
2. **Act** on the selected node (examine, modify, document, test)
3. **Observe** the outcome (test result, new edges discovered, suggestions emitted)
4. **Update beliefs** by writing perspectives — this changes the entropy landscape
5. **Repeat** — the updated perspectives change G for all nodes, shifting the priority queue

```
Agent loop:
    candidates = query nodes in scope, ordered by G
    target = candidates[0]
    outcome = act(target)           -- modify, document, test
    update_perspectives(target, outcome)  -- belief updating
    -- G landscape has shifted; next iteration picks a new best target
```

The agent's perspectives are its approximate posterior — its current best beliefs about the codebase. Minimizing G drives the agent to update this posterior toward the true structure of the code, while simultaneously pursuing goals. Perception and action unified under one objective.

### 20.6 Precision as Koi Value

In Active Inference, **precision** weights the reliability of different information channels. High precision means "trust this signal." In kerai, precision maps to economic value:

- A node with high Koi throughput (frequently traded, referenced in successful auction settlements) has high precision — the market has validated its value.
- A node with no economic activity has low precision — it might be important or it might be dead code.
- An agent's perspective carries precision proportional to that agent's track record. An agent with high pass rates (Plan 09) has high-precision perspectives.

Precision weighting modifies the entropy calculation:

```sql
-- Precision-weighted entropy: high-precision nodes have lower effective entropy
-- (the market has already reduced uncertainty about them)
node_entropy(n) / (1.0 + precision(n))
```

Where `precision(n)` can be derived from:
- Reward log: total Koi minted for operations involving this node
- Perspective consensus: how many agents agree (low variance = high precision)
- Test coverage: nodes exercised by passing tests have higher precision

This creates a feedback loop: valuable knowledge attracts economic activity, which increases precision, which lowers entropy, which makes agents focus elsewhere — until external changes (new tasks, code modifications) raise entropy again.

### 20.7 Markov Blankets for Scope Boundaries

Active Inference defines a **Markov blanket** as the set of variables mediating all interactions between a system and its environment. In kerai, a module's Markov blanket is its public interface:

```sql
-- Compute the Markov blanket of a module (its public API surface)
SELECT n.id, n.kind, n.content
FROM kerai.nodes n
WHERE n.parent_id = '<module-node-id>'
AND n.metadata->>'visibility' = 'pub';
```

Markov blankets define natural scope boundaries for agent work. An agent assigned to a module should:
- Freely modify internal nodes (within the blanket)
- Only modify blanket nodes (public API) with higher justification (lower G threshold)
- Treat nodes outside the blanket as observations, not actions

This maps directly to Active Inference's distinction between internal states (modifiable), blanket states (the interface), and external states (observable only). Agents that respect Markov blanket boundaries produce changes with fewer cross-module side effects.

### 20.8 Message Passing for Change Propagation

When an agent modifies a node, prediction errors propagate through edges. This is the "message passing" from Chapter 5 of Parr et al. — descending predictions meet ascending errors.

In kerai terms: a change to function `foo` should raise the entropy of:
- Callers of `foo` (ascending: "my dependency changed, I might need to adapt")
- Callees of `foo` (descending: "my caller's expectations may have shifted")
- Test nodes covering `foo` (lateral: "my assertions may no longer hold")

This is implemented as a trigger or post-action step:

```sql
-- After modifying node X, propagate entropy increase to connected nodes
-- This ensures agents are drawn to examine impacted code
CREATE OR REPLACE FUNCTION kerai.propagate_change_entropy(changed_id uuid)
RETURNS void AS $$
    -- Mark connected nodes as "entropy increased" by updating a lightweight
    -- change_epoch counter in metadata, which the entropy function reads
    UPDATE kerai.nodes
    SET metadata = jsonb_set(
        metadata,
        '{change_epoch}',
        to_jsonb(COALESCE((metadata->>'change_epoch')::int, 0) + 1)
    )
    WHERE id IN (
        SELECT target_id FROM kerai.edges WHERE source_id = changed_id
        UNION
        SELECT source_id FROM kerai.edges WHERE target_id = changed_id
    );
$$ LANGUAGE sql VOLATILE;
```

This creates a wavefront of elevated entropy that attracts agent attention to the blast radius of a change — without any explicit "notify" mechanism. The entropy landscape itself is the communication channel.

### 20.9 MicroGPT Integration

Plan 13's MicroGPT learns to predict the next relevant node from graph walks. Active Inference provides the training objective:

- **Training signal**: The model should minimize prediction error over sequences of *agent-selected* nodes — not random walks, but walks that follow the G-minimizing policy. This trains the model to internalize the expected free energy landscape.
- **Inference use**: Given a partial walk (the agent's recent history), MicroGPT predicts which node the agent should examine next. This prediction IS the agent's approximate posterior over future states — exactly what Active Inference calls a policy.
- **Learned precision**: The model's confidence in its next-node prediction serves as a learned precision weight. High-confidence predictions narrow the agent's focus; low-confidence predictions broaden exploration.

Training walks become:

```sql
-- Generate training sequences from agent behavior (G-minimizing walks)
SELECT kerai.generate_walk(
    'agent_history',     -- new walk type: replay agent's actual node visits
    '<agent-id>',
    16                   -- context length
);
```

The model learns the structure of productive agent behavior, not just graph topology. Over time, it becomes a compressed representation of the expected free energy landscape — a learned generative model of the generative model.

## Schema Changes

Minimal. The scoring functions operate over existing tables. One optional addition:

```sql
-- Cache expensive entropy computations (refreshed periodically or on change)
CREATE TABLE kerai.entropy_cache (
    node_id     uuid PRIMARY KEY REFERENCES kerai.nodes(id) ON DELETE CASCADE,
    entropy     float NOT NULL,
    pragmatic   float NOT NULL,
    efe         float NOT NULL,      -- expected free energy
    computed_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_entropy_cache_efe ON kerai.entropy_cache(efe);

-- Refresh function (called periodically or after bulk changes)
CREATE OR REPLACE FUNCTION kerai.refresh_entropy_cache()
RETURNS integer AS $$
DECLARE
    refreshed integer;
BEGIN
    INSERT INTO kerai.entropy_cache (node_id, entropy, pragmatic, efe, computed_at)
    SELECT
        n.id,
        kerai.node_entropy(n.id),
        kerai.node_pragmatic_value(n.id),
        kerai.expected_free_energy(n.id),
        now()
    FROM kerai.nodes n
    WHERE n.kind IN ('fn', 'struct', 'enum', 'module', 'file', 'document',
                     'trait', 'impl', 'const', 'type_alias')
    ON CONFLICT (node_id) DO UPDATE SET
        entropy = EXCLUDED.entropy,
        pragmatic = EXCLUDED.pragmatic,
        efe = EXCLUDED.efe,
        computed_at = EXCLUDED.computed_at;

    GET DIAGNOSTICS refreshed = ROW_COUNT;
    RETURN refreshed;
END;
$$ LANGUAGE plpgsql;
```

## Implementation Steps

1. **Implement `node_entropy`** as a SQL function over existing schema. Test against the 55K+ nodes currently in the database.
2. **Implement `node_pragmatic_value`** as a SQL function. Requires Plan 09 tasks table to be populated.
3. **Implement `expected_free_energy`** as the composite. Verify that G ranking produces sensible orderings (well-documented functions have low epistemic value; nodes in task scope have high pragmatic value).
4. **Add `entropy_cache`** table for performance. Implement `refresh_entropy_cache`.
5. **Add `propagate_change_entropy`** trigger to edges/nodes modification.
6. **Integrate with agent loop** — add a `kerai.select_next_target(agent_id, scope_id, limit)` convenience function that returns the top-N lowest-G nodes in scope.
7. **Add `agent_history`** walk type to Plan 13 for MicroGPT training on G-minimizing behavior.
8. **Tune weights** — use agent pass rates (Plan 09) as ground truth to adjust entropy component weights.

## Decisions to Make

- **Entropy component weights:** The initial weights (0.3 for edge sparsity, 0.25 for perspective sparsity, etc.) are guesses. Should these be fixed, configurable per-agent, or learned by MicroGPT? Proposed: configurable per-agent as JSONB in `agents.config`, with MicroGPT learning optimal weights as a future step.
- **Cache invalidation:** Should `entropy_cache` refresh on every change, periodically, or on demand? Proposed: periodically (every N minutes via `pg_cron` or agent-triggered), plus on-demand after bulk operations like `parallel_parse`.
- **Scope of G computation:** Computing G for all 55K+ nodes is expensive. Should agents only score nodes within their task scope, or maintain a global view? Proposed: scope-limited by default, with a global refresh for unscoped agents.
- **Exploration floor:** Should there be a minimum epistemic weight to prevent agents from becoming purely goal-directed? Active Inference says yes — even goal-directed behavior requires uncertainty reduction. Proposed: `epistemic_weight >= 0.1` floor.
- **Multi-step planning:** This plan scores single actions. Active Inference supports deep planning (evaluating sequences of actions). Should agents plan multi-step sequences? Proposed: defer to MicroGPT — the learned model implicitly captures multi-step value through its context window.

## Relationship to Other Plans

- **Plan 08 (Perspectives):** Perspectives ARE the agent's beliefs. G scoring reads them; the agent loop writes them. This plan turns perspectives from passive records into an active inference substrate.
- **Plan 09 (Swarms):** This plan answers the question Plan 09 defers: "how does an agent decide what to work on?" The expected free energy score replaces external orchestration with intrinsic motivation.
- **Plan 13 (MicroGPT):** Active Inference provides MicroGPT's training objective. Instead of learning arbitrary graph walks, the model learns to predict G-minimizing agent behavior — becoming a neural approximation of the expected free energy landscape.
- **Plan 14 (ZK Currency):** Precision weighting connects Koi value to information reliability. Nodes with economic activity have higher precision, lower effective entropy. The market IS the precision mechanism.

## Out of Scope

- Full Bayesian belief propagation (loopy belief propagation on the node graph — computationally expensive, unclear benefit over the heuristic approach)
- Continuous-time Active Inference (Chapter 8 — relevant for real-time editor integration in Plan 12, but not for batch agent task selection)
- Embodied Active Inference (the sensorimotor loop described in Chapter 3 — kerai agents are disembodied; their "body" is the codebase)
- Formal generative model specification (defining the full joint probability P(o,s) over the node graph — the SQL heuristics capture the structure without requiring explicit probabilistic programming)

## References

- Parr, T., Pezzulo, G., & Friston, K.J. (2022). *Active Inference: The Free Energy Principle in Mind, Brain, and Behavior.* MIT Press. Chapter 7 §7.3 (expected free energy decomposition), Chapter 5 (message passing), Chapter 3 §3.3 (Markov blankets).
- Friston, K.J. et al. (2015). Active inference and epistemic value. *Cognitive Neuroscience*, 6(4), 187–214.
- Da Costa, L. et al. (2020). Active inference on discrete state-spaces: A synthesis. *Journal of Mathematical Psychology*, 99, 102447.
