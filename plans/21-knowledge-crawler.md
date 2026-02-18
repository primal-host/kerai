# Plan 21: Knowledge Crawler — Autonomous Corpus Building

*Depends on: Plan 12 (Knowledge Editor — markdown parsing), Plan 19 (Repository Ingestion — file walker), Plan 20 (Active Inference — scoring)*
*Enables: —*

## Goal

Build kerai's first autonomous agent: a background crawler that examines ingested documents, extracts external references, scores them by expected free energy, fetches publicly available copies, and ingests them — expanding the knowledge corpus along the citation graph at a steady, sustainable pace.

The crawler is Active Inference in miniature. It has a generative model (the existing node graph), beliefs (which references lead to valuable knowledge), and a single objective (minimize expected free energy by resolving the highest-value unknowns). It perceives (extracts references), plans (scores candidates), acts (fetches and ingests), and updates beliefs (marks references resolved, records what it found). The corpus grows organically along paths of highest information gain.

## The Crawl Loop

```
loop:
    1. EXTRACT  — scan ingested documents for unresolved references
    2. SCORE    — rank references by expected free energy (Plan 20)
    3. SELECT   — pick the highest-scoring unresolved reference
    4. SEARCH   — query external sources for a public copy
    5. FETCH    — download the document (PDF, HTML, markdown)
    6. CONVERT  — transform to markdown (pdftotext, html2md, or passthrough)
    7. INGEST   — parse_markdown → new nodes and edges
    8. LINK     — connect ingested document back to citing reference
    9. UPDATE   — mark reference resolved, write perspectives
   10. SLEEP    — wait N seconds (configurable rate limit)
   11. goto 1
```

Each cycle produces new nodes. Those nodes contain their own references — the frontier expands. The entropy landscape shifts with each ingestion, naturally steering the crawler toward under-represented areas without explicit programming.

## Deliverables

### 21.1 Reference Extraction

Scan paragraph nodes for extractable references. Three classes:

**a) URLs and DOIs:**
```
https://arxiv.org/abs/2003.04035
doi:10.1016/j.jmp.2020.102447
```

Regex extraction from paragraph content. Direct fetch targets.

**b) Author-year citations:**
```
(Friston 2010)
(Parr & Friston, 2019)
(Da Costa et al., 2020)
Helmholtz (1866)
```

Pattern-matched from text. Resolved via Semantic Scholar, CrossRef, or Google Scholar API.

**c) Named works and standards:**
```
"Bayesian brain hypothesis"
"predictive coding"
TOTE (Test, Operate, Test, Exit)
```

Extracted as concept references. Lower priority — resolved via web search with the concept name plus surrounding context.

Each extracted reference becomes a node:

```sql
-- Reference node
INSERT INTO kerai.nodes (id, instance_id, kind, content, metadata)
VALUES (
    gen_random_uuid(),
    <crawler_instance_id>,
    'reference',
    'Friston, K.J. (2010). The free-energy principle: a unified brain theory?',
    '{
        "ref_type": "citation",
        "authors": ["Friston, K.J."],
        "year": 2010,
        "doi": "10.1038/nrn2787",
        "status": "unresolved",
        "cite_count": 3,
        "first_seen_in": "<document-node-id>"
    }'
);
```

With an edge back to the citing paragraph:

```sql
-- Citation edge: paragraph cites reference
INSERT INTO kerai.edges (id, source_id, target_id, relation, metadata)
VALUES (
    gen_random_uuid(),
    <paragraph_node_id>,
    <reference_node_id>,
    'cites',
    '{"context": "first 100 chars of surrounding text..."}'
);
```

### 21.2 Reference Scoring

Each unresolved reference is scored by expected free energy. The epistemic and pragmatic components for a reference:

**Epistemic value (what will we learn?):**
- **Citation frequency:** A reference cited in 5 paragraphs across 3 chapters has more information value than one mentioned once. `cite_count` in the reference metadata.
- **Topic entropy:** If the reference appears in a region of the graph with high average node entropy (poorly understood area), resolving it has higher epistemic value.
- **Novelty:** References to authors/topics not yet represented in the corpus have higher information gain than references to authors with 10 papers already ingested.

**Pragmatic value (does this serve a goal?):**
- **Task alignment:** If an open task or bounty relates to the reference's topic, resolving it has pragmatic value.
- **Agent demand:** If other agents have recorded high-weight perspectives on the citing paragraphs, the reference is pragmatically important.
- **Chain depth:** References from recently ingested documents (the frontier) have higher pragmatic value than references from the original seed — they extend the corpus into new territory.

```sql
-- Score an unresolved reference
CREATE OR REPLACE FUNCTION kerai.score_reference(ref_id uuid)
RETURNS float AS $$
    SELECT
        -- Epistemic: citation frequency (log-scaled)
        ln(1 + COALESCE((SELECT count(*) FROM kerai.edges
                          WHERE target_id = ref_id AND relation = 'cites'), 0)) * 0.4

        -- Epistemic: topic entropy of citing region
        + COALESCE((SELECT avg(kerai.node_entropy(e.source_id))
                     FROM kerai.edges e WHERE e.target_id = ref_id
                     AND e.relation = 'cites'), 0) * 0.3

        -- Epistemic: novelty (author not yet in corpus)
        + CASE WHEN NOT EXISTS(
            SELECT 1 FROM kerai.nodes n2
            WHERE n2.kind = 'document'
            AND n2.metadata->>'authors' IS NOT NULL
            AND n2.metadata->>'authors' ILIKE
                '%' || (SELECT metadata->>'authors' FROM kerai.nodes WHERE id = ref_id) || '%'
          ) THEN 1.0 ELSE 0.1 END * 0.2

        -- Pragmatic: chain depth bonus (frontier exploration)
        + COALESCE((SELECT (metadata->>'chain_depth')::float
                    FROM kerai.nodes WHERE id = ref_id), 0) * 0.1
    ;
$$ LANGUAGE sql STABLE;
```

The crawler selects: `ORDER BY kerai.score_reference(id) DESC LIMIT 1`.

### 21.3 External Source Resolution

The crawler searches for public copies using a priority stack of sources:

| Source | Best for | API | Rate limit |
|---|---|---|---|
| Semantic Scholar | Academic papers | `api.semanticscholar.org/graph/v1` | 100/5min (unauthenticated) |
| CrossRef | DOI resolution | `api.crossref.org/works/{doi}` | 50/sec (polite pool) |
| Unpaywall | Open access PDFs | `api.unpaywall.org/v2/{doi}` | 100K/day |
| Internet Archive | Books, historical | `archive.org/download/{id}` | Generous |
| Direct URL | URLs in text | HTTP fetch | Respect robots.txt |

Resolution strategy per reference type:
1. **DOI present** → CrossRef for metadata → Unpaywall for OA PDF → Semantic Scholar for abstract
2. **Author-year citation** → Semantic Scholar search → if OA, fetch PDF
3. **URL** → direct fetch → convert based on content-type
4. **Named concept** → web search → filter for substantial, freely available sources

Only ingest content that is:
- Open access (Creative Commons, public domain, OA journal)
- Freely available (no paywall, no login required)
- Substantial (>1 page of actual content, not just an abstract)

### 21.4 Document Conversion Pipeline

Downloaded content needs conversion to markdown before ingestion:

```
PDF  → pdftotext → chapter splitting (heuristic) → markdown
HTML → html2md (or readability extraction) → markdown
EPUB → unzip → HTML extraction → markdown
TXT  → direct passthrough as markdown
MD   → direct passthrough
```

Each converted document gets metadata tracking provenance:

```json
{
    "source_url": "https://archive.org/download/...",
    "source_type": "pdf",
    "fetched_at": "2026-02-18T21:30:00Z",
    "license": "CC-BY-4.0",
    "chain_depth": 2,
    "chain_root": "<active-inference-doc-id>",
    "resolved_from": "<reference-node-id>",
    "crawler_agent": "<agent-id>"
}
```

### 21.5 Provenance Edges

After ingestion, the crawler creates edges that trace the citation chain:

```sql
-- Reference resolves to the ingested document
INSERT INTO kerai.edges (source_id, target_id, relation, metadata)
VALUES (<reference_node_id>, <new_document_node_id>, 'resolves_to',
        '{"fetched_at": "...", "source_url": "..."}');
```

This creates a queryable citation graph:

```sql
-- Trace the citation chain from a seed document
WITH RECURSIVE chain AS (
    SELECT d.id, d.content, 0 as depth
    FROM kerai.nodes d WHERE d.id = '<seed-document-id>'
    UNION ALL
    SELECT d2.id, d2.content, c.depth + 1
    FROM chain c
    JOIN kerai.edges cite ON cite.source_id IN (
        SELECT id FROM kerai.nodes WHERE parent_id = c.id
    ) AND cite.relation = 'cites'
    JOIN kerai.edges resolve ON resolve.source_id = cite.target_id
        AND resolve.relation = 'resolves_to'
    JOIN kerai.nodes d2 ON d2.id = resolve.target_id
    WHERE c.depth < 5
)
SELECT DISTINCT id, content, depth FROM chain ORDER BY depth;
```

### 21.6 The Crawler Agent

The crawler is registered as a kerai agent (Plan 08/09):

```sql
-- Register the crawler agent
SELECT kerai.register_agent('kerai_crawl', 'tool', NULL,
    '{"type": "crawler", "rate_limit_seconds": 60, "max_chain_depth": 5}'::jsonb);
```

It earns Koi for successful ingestion via the existing `mint_reward` mechanism:

```sql
-- Reward schedule entry for crawl ingestion
INSERT INTO kerai.reward_schedule (work_type, reward_nkoi)
VALUES ('crawl_ingest', 100000000);  -- 0.1 Koi per successfully ingested document
```

Agent configuration (stored in `agents.config`):

| Setting | Default | Description |
|---|---|---|
| `rate_limit_seconds` | 60 | Minimum seconds between fetch attempts |
| `max_chain_depth` | 5 | Maximum citation chain depth from seed |
| `min_score` | 0.5 | Minimum reference score to attempt resolution |
| `max_pdf_mb` | 50 | Maximum PDF size to download |
| `allowed_licenses` | `["cc-by", "cc-by-sa", "cc-by-nc", "public-domain", "OA"]` | Only ingest these |
| `epistemic_weight` | 1.0 | Weight for epistemic value in scoring |
| `pragmatic_weight` | 0.5 | Weight for pragmatic value (crawler is exploration-biased) |

### 21.7 Background Execution

Three options for continuous operation, in order of preference:

**a) pg_cron (simplest):**
```sql
-- Run one crawl cycle every 60 seconds
SELECT cron.schedule('kerai_crawl', '* * * * *',
    $$SELECT kerai.crawl_cycle()$$);
```

**b) pg_background worker:**
```sql
-- Launch a long-running crawl worker
SELECT kerai.start_crawler(rate_limit_seconds := 60);
```

The worker runs in a loop inside a pg_background session, sleeping between cycles. Stopped via `kerai.stop_crawler()`.

**c) External process:**
A standalone Rust binary (in the CLI crate) that connects to Postgres, runs the loop, and calls the web APIs. This is the most flexible — it can use async HTTP, respect robots.txt, handle retries — but requires a process outside Postgres.

Proposed: start with **(c)** external process, since web fetching (HTTP clients, TLS, robots.txt, API rate limiting) is awkward inside a Postgres extension. The process uses the same `psql` connection as any kerai client. The scoring, reference extraction, and ingestion happen via SQL; only the fetch and conversion happen externally.

### 21.8 Crawl State Machine

Each reference progresses through states:

```
unresolved → searching → found → fetching → converting → ingesting → resolved
                       → not_found (no public copy available)
                       → skipped (license, size, or content-type exclusion)
                       → failed (fetch or conversion error)
```

State transitions are recorded in the reference node's metadata:

```sql
UPDATE kerai.nodes SET metadata = jsonb_set(
    jsonb_set(metadata, '{status}', '"resolved"'),
    '{resolved_at}', to_jsonb(now()::text)
)
WHERE id = <reference_node_id>;
```

Failed references get a retry count and exponential backoff:

```json
{"status": "failed", "retries": 2, "next_retry_after": "2026-02-19T03:00:00Z",
 "last_error": "HTTP 503 Service Unavailable"}
```

### 21.9 Corpus Statistics

Dashboard queries for monitoring corpus growth:

```sql
-- Corpus overview
SELECT
    (SELECT count(*) FROM kerai.nodes WHERE kind = 'document') as documents,
    (SELECT count(*) FROM kerai.nodes WHERE kind = 'reference') as references,
    (SELECT count(*) FROM kerai.nodes WHERE kind = 'reference'
     AND metadata->>'status' = 'resolved') as resolved,
    (SELECT count(*) FROM kerai.nodes WHERE kind = 'reference'
     AND metadata->>'status' = 'unresolved') as frontier,
    (SELECT count(*) FROM kerai.nodes) as total_nodes;

-- Citation chain from a seed
SELECT metadata->>'chain_depth' as depth, count(*)
FROM kerai.nodes WHERE kind = 'document'
AND metadata->>'chain_root' IS NOT NULL
GROUP BY metadata->>'chain_depth' ORDER BY depth;

-- Top unresolved references (crawler's priority queue)
SELECT content, metadata->>'cite_count' as citations,
       kerai.score_reference(id) as score
FROM kerai.nodes
WHERE kind = 'reference' AND metadata->>'status' = 'unresolved'
ORDER BY kerai.score_reference(id) DESC LIMIT 20;

-- Crawler activity log
SELECT date_trunc('hour', (metadata->>'resolved_at')::timestamptz) as hour,
       count(*) as ingested
FROM kerai.nodes WHERE kind = 'reference'
AND metadata->>'status' = 'resolved'
GROUP BY hour ORDER BY hour DESC LIMIT 24;
```

## Implementation Steps

1. **Reference extractor** — SQL function + Rust helper to scan paragraph nodes for URLs, DOIs, and author-year citations. Create `reference` nodes and `cites` edges.
2. **Score function** — Implement `kerai.score_reference()` over existing schema. Test against Active Inference book references.
3. **External resolver** — CLI binary that takes a reference node ID, searches Semantic Scholar / Unpaywall / Internet Archive, returns a download URL.
4. **Conversion pipeline** — `pdftotext` + chapter splitter (reuse the approach from Active Inference ingestion). HTML via `readability` or similar.
5. **Ingest + link** — Call `parse_markdown`, create `resolves_to` edge, update reference status.
6. **Crawl loop** — CLI command `kerai crawl --rate 60 --max-depth 5` that runs continuously.
7. **Agent registration** — Register `kerai_crawl` agent, wire up Koi rewards.
8. **Monitoring** — Corpus statistics queries, optional `pg_notify` for real-time progress.

## First Test: Active Inference Citation Graph

The ingested Active Inference book is the perfect seed. Expected behavior:

1. Crawler extracts ~200 references from the book's 2,909 nodes
2. Scoring surfaces high-value targets: Friston (2010) "The free-energy principle: a unified brain theory?" (cited repeatedly), Helmholtz, Bayesian brain, predictive coding papers
3. Semantic Scholar finds OA copies of many neuroscience papers
4. First wave: ~30-50 papers ingested (the open access ones)
5. Those papers cite further work → second wave of references
6. After a few hours at 1/minute pace: a corpus of 100+ documents, 50K+ new nodes, all interconnected by citation edges

The corpus becomes a queryable knowledge graph of Active Inference and its intellectual foundations — built autonomously from a single seed document.

## Decisions to Make

- **Deduplication:** The same paper may be cited with slightly different author-year strings across documents. How aggressively to deduplicate? Proposed: DOI match first (exact), then fuzzy title+year match via trigram similarity (`pg_trgm`).
- **Abstract-only vs full text:** When only an abstract is freely available, should the crawler ingest it? Proposed: yes, as a thin document node with `metadata.content_type = 'abstract'`. Better than nothing — it still provides headings, key terms, and citation context.
- **Copyright caution:** Some papers are technically OA but have restrictive licenses (CC-BY-NC-ND). Should the crawler ingest these? Proposed: yes for personal/research use, with license recorded in metadata. The crawler respects `allowed_licenses` config.
- **Recursive depth:** Should the crawler follow citations indefinitely? Proposed: configurable `max_chain_depth` (default 5). The expected free energy scoring naturally deprioritizes distant references anyway — diminishing epistemic returns at depth.
- **Multiple seeds:** Should the crawler support multiple seed documents simultaneously? Proposed: yes. Each seed gets its own `chain_root`. The scoring function naturally interleaves work across seeds based on which frontier has the highest expected free energy.

## Relationship to Other Plans

- **Plan 12 (Knowledge Editor):** The crawler is the automated complement to manual document ingestion. Human editors add seed documents; the crawler expands them.
- **Plan 19 (Repository Ingestion):** Shares the file walker and conversion pipeline patterns. Could extend to crawl code repository references (GitHub links in docs, dependency documentation).
- **Plan 20 (Active Inference):** The crawler IS an Active Inference agent — the first one. Its behavior validates the expected free energy scoring framework on a real task.
- **Plan 08 (Perspectives):** The crawler writes perspectives on ingested content — marking what it found relevant, what was surprisingly connected, what led to dead ends. Other agents can read the crawler's perspective to understand the corpus.
- **Plan 09 (Agent Swarms):** The crawler is a single agent, but the pattern extends to swarms of crawlers — each seeded with a different document or topic, collectively building the corpus. Market dynamics (Plan 09 §9.6) apply: crawlers that find valuable content earn more Koi.

## Out of Scope

- Full-text search over crawled content (Plan 07 already provides FTS via `context_search`)
- Summarization or synthesis of crawled papers (future work — could use MicroGPT or external LLM)
- Crawling non-text content (images, datasets, videos)
- Authenticated access to paywalled content (only public/OA sources)
- Real-time web monitoring (watching for new papers on a topic — future extension)
