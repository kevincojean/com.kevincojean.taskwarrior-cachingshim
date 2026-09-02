# taskwarrior-caching-shim ledger

## 2026-09-02 - strace gate failure: clear-on-mutation defeats warm cache

Measured with strace on taskwarrior-tui (isolated TASKDATA, scripted pty,
release shim via TASKWARRIOR_TUI_TASKWARRIOR_CLI):

- Startup `_get`/`show` calls HIT the cache correctly (0 real-task execs).
- Every `add` triggers `clear_cache_on_mutation_operation=true` -> full wipe.
- The TUI re-renders immediately after the add, re-running the same
  6 config reads -> all MISS -> re-exec real task.
- Warm add flow: 11 real-task execs. Gate is <=4. FAIL.
- Root cause is the PLAN, not the impl: the What section targets <=4 spawns
  and names context/--version as cached, but the shipped config only enables
  _get/show and forces clear=true. With clear=true the gate is
  mathematically unreachable. Momus review missed it; each section is
  individually coherent.

## 2026-09-02 - invalidation model: key vs flag

Two independent invalidation problems, previously conflated:

| Change | Affected cached shapes | Correct mechanism |
|---|---|---|
| taskrc edited / context switched | all config reads | cache key already includes taskrc include-chain mtime -> automatic miss |
| task data changed (add/done/...) | NOTHING in v1 (all cached shapes are config reads) | n/a in v1; needs explicit clearing in v2 when summary/export get cached |

Decision: kill the global boolean. Replace with an explicit wiper list:
`clearing_commands.<cmd>=true`, unlisted = false, EMPTY in v1.
Rejected alternative: "default-wipe except whitelisted" - breaks because the
TUI's passthrough traffic is mostly DATA READS (export, summary, timesheet,
count) which must not wipe. Writes are a small finite set; reads are
open-ended. The list must contain wipers, not exceptions.

## 2026-09-02 - context arity trap

`task context` bare = LIST (read). `task context <name>` = SWITCH (edits
taskrc, a WRITE). `task context <name> <filter>` = DEFINE (also writes).

classify() matched only the first non-rc word, so all forms shared
CommandClass::Context. Enabling cached_commands.context=true as-is gives a
SILENT-Failure bug: switching work->home->work hits the cache on the second
`context work`, replays old output, NEVER execs real task, taskrc untouched.
TUI shows "now on work", data says home. mtime key does not save this:
the cached-hit path skips the exec, so nothing touches taskrc.

Fix: arity-aware CommandClass variants. ListContexts = bare `context` only
(zero trailing args after rc.* overrides). Write forms route to
Write/Passthrough, structurally unable to reach the cache module.
`_get`/`show` need no split - every form is a pure config read.

## 2026-09-02 - RW set is NOT the complete write predicate (librarian, taskwarrior source)

Verified against GothenburgBitFactory/taskwarrior tags v3.4.2/v2.6.2/v1.9.4:

- `rc.commands.read`/`commands.write` config keys NEVER existed (grep: zero hits
  in all three tags). The real taxonomy is the per-command `_read_only` flag,
  surfaced in `task commands` R/W column.
- Authoritative "writes task data" predicate (Context.cpp:683 v3.4.2):
  `read_write = (gc && (needs_gc || needs_recur_update)) || !read_only`
- So export/summary/count/timesheet are RO-flagged but run GC/recurrence and
  DO write task data when rc.gc=on (default). The 19-word RW list alone is
  incomplete as a v2 wiper seed. Full seed: `RW OR (gc_on AND (GC OR Recur))` -
  all three columns are visible in `task commands`.
- taskrc writers are orthogonal to _read_only: only `context <name>`,
  `config <n> <v>`, `news` edit taskrc, and all are RO-flagged.
  Covered automatically by the taskrc-mtime key. No list needed.
- v1 impact: none (cached shapes are pure config reads; wiper list empty).
- v2 impact: prefer data-file mtime in the key over wiper lists - closes
  aliases AND out-of-band writes AND GC's own writes. Paradox to resolve
  later: export runs GC -> rewrites data -> invalidates its own cache entry.
- Invariant that survived: correctness is bet on mtime keys, never on
  enumeration completeness.

## 2026-09-02 - planned config + gate math

Shipped config becomes: cached_commands._get/show/context/version=true,
clear_cache_on_mutation_operation removed, clearing_commands empty.
Expected warm add flow after fix: exactly 4 real execs
(add, export, summary, timesheet) - matches plan's ~5.7s target.

Acceptance criteria (Given/When/Then, real task binary, no mocks, isolated
TASKDATA/TASKRC):
1. `context work` run twice -> both exec, zero HIT lines, taskrc really changed.
2. bare `context` twice -> 2nd HIT, byte-identical.
3. warm cache + `add` passes through -> cache survives, next `_get` HITs.
4. strace gate: warm add flow <=4 real execs, <=6s.

## 2026-09-02 - rev 13 landed: arity-aware classify, clear flag deleted

Implemented steps 1-3 of rev 13 (RED then GREEN):

- Config: dropped `clear_cache_on_mutation_operation`; enabled
  `cached_commands.context` and `cached_commands.version`.
- CommandClass: GetConfig/ShowConfig/ListContexts/Version/Passthrough.
 Mutation deleted.

- classify() is arity-aware: bare `context` -> ListContexts (cached),
  `context <name>` -> Passthrough (a write, structurally cannot reach cache);
  bare `--version` -> Version (cached), `--version <arg>` -> Passthrough.

- `Cache::clear()` deleted (v1 has no wipers; mtime key covers the rest).
- e2e: deleted mutation-clears-cache test; moved context/--version vectors into
  cacheable set; added `context_write_form_never_cached` + `add_preserves_config_cache`.
- RED confirmed on both new tests; GREEN after dispatcher changes.

- Key design note: `context <name>` writes taskrc -> changes mtime -> even the
  OLD classify() would MISS on 2nd run. The real red assertion was zero
  MISS lines too (cache module must never be reached at all for the write form).

## 2026-09-02 - rev 13 strace gate: PASS (11 -> 4 execs)

Independent re-measurement (isolated TASKDATA/TASKRC/HOME, one strace per
add flow, gate on the SECOND warm flow, release build):

- Run A baseline (real task as CLI): add-window = 11 real execs.
- Run B shim, warm flow: add-window = 4 real execs
  (add, export, summary, timesheet), 0.212s. All config reads
  (_get x2, show x4, context, --version) = cache HITs, 0 real execs.
- add did NOT clear cache (rev 13 fix confirmed): 8 entries survived.
- Gate (<=4 execs AND <=6s): PASS with margin.
- Remaining 4 execs are exactly the plan's v1 cache exclusions.
- cargo test: 8 passed (3 unit + 5 e2e incl. context_write_form_never_cached
  and add_preserves_config_cache).
- Not committed. Next per plan: half-day daily-drive before done.
