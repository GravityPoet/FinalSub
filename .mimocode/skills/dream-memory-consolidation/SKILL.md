---
name: dream-memory-consolidation
description: Run a dream memory consolidation pass — read recent session trajectories from the mimocode DB and memory files, extract durable knowledge, and consolidate into project MEMORY.md.
---

# Dream Memory Consolidation

Read recent session trajectories and memory files, extract durable verified information, and consolidate into the project's `MEMORY.md`.

## Inputs

- `$ARGUMENTS` — optional focus constraints (e.g. "only sessions from last 7 days", "focus on ChordVox UI decisions")

## Data Sources (read-only)

1. **Trajectory DB**: `<DATA>/mimocode.db` (SQLite, read-only)
2. **Project memory**: `<DATA>/memory/projects/<project_id>/MEMORY.md`
3. **Session checkpoints**: `<DATA>/memory/sessions/<session_id>/checkpoint.md`
4. **Session notes**: `<DATA>/memory/sessions/<session_id>/notes.md`
5. **Global memory**: `<DATA>/memory/global/MEMORY.md`

`<DATA>` = `/Users/moonlitpoet/.local/share/mimocode`

## Procedure

### Step 1 — Locate project context

```bash
# Find project memory directory
ls -la <DATA>/memory/projects/

# Identify current project ID from the working directory
sqlite3 <DATA>/mimocode.db "
  SELECT id, directory FROM project WHERE directory LIKE '%<current_project>%';
"
```

### Step 2 — Inventory recent sessions

```bash
# List non-checkpoint sessions from the past 30 days for this project
sqlite3 <DATA>/mimocode.db "
  SELECT s.id, s.title, datetime(s.time_created/1000, 'unixepoch') as created
  FROM session s
  WHERE s.directory LIKE '%<project_path>%'
    AND s.time_created > (strftime('%s', 'now', '-30 days') * 1000)
    AND s.title NOT LIKE 'checkpoint-writer%'
  ORDER BY s.time_created DESC;
"
```

### Step 3 — Read existing memory and checkpoints

- Read the project `MEMORY.md` to understand what's already recorded.
- Find recent checkpoints (last 7–14 days) and read them:
  ```bash
  find <DATA>/memory/sessions -name "checkpoint.md" -mtime -14 | sort
  ```
- Read each relevant checkpoint for session summaries, decisions, and discoveries.

### Step 4 — Query trajectory for deeper evidence

For sessions that look significant, query the raw message/part tables:

```sql
-- Get user messages from a specific session
SELECT datetime(m.time_created/1000, 'unixepoch') as time,
       json_extract(p.data, '$.text') as text
FROM message m
JOIN part p ON p.message_id = m.id
WHERE m.session_id = '<session_id>'
  AND json_extract(m.data, '$.role') = 'user'
  AND json_extract(p.data, '$.type') = 'text'
ORDER BY m.time_created;

-- Get tool usage patterns
SELECT json_extract(p.data, '$.tool') as tool, count(*) as n
FROM message m
JOIN part p ON p.message_id = m.id
WHERE m.session_id = '<session_id>'
  AND json_extract(p.data, '$.type') = 'tool'
GROUP BY tool ORDER BY n DESC;
```

### Step 5 — Extract and consolidate

Identify durable facts worth persisting:

- **Architecture decisions** — choices made with rationale
- **Rules** — hard constraints the user stated
- **Discovered knowledge** — verified facts about the codebase, dependencies, or environment
- **Failed approaches** — what was tried and didn't work (avoids repeating)
- **Live resources** — URLs, paths, credentials locations, service endpoints still active

### Step 6 — Update MEMORY.md

Edit the project `MEMORY.md` under the appropriate sections:

- `## Architecture decisions` — for structural/design choices
- `## Rules` — for user-stated constraints
- `## Discovered durable knowledge` — for verified facts
- `## Dead ends` — for failed approaches worth remembering

**Rules:**
- Only consolidate **verified** information (confirmed by code, DB, or explicit user statement)
- Do not duplicate what's already in MEMORY.md
- Do not record transient state (temporary bugs, in-progress work)
- Keep entries concise: fact + evidence source (session ID or file path)

## Output

Report what was added/updated in MEMORY.md, with a count of sessions reviewed and facts consolidated.

## Stop Condition

MEMORY.md is updated with new durable facts, or no new durable facts were found (report "nothing to consolidate").
