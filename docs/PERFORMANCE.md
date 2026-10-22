# Performance

What the player costs when the library is big, and what is promised about it.

The numbers below are measured on the production code paths, on a library of
**100,000 tracks**, in a **release** build. They are budgets: the CI asserts
them, so a change that makes the player an order of magnitude slower cannot
land unnoticed.

## How to reproduce

```bash
# The budgets, as CI runs them.
cargo test --release -p ui --test library_budget -- --nocapture --test-threads=1

# The same operations under the benchmark harness.
cargo bench -p ui --bench library_100k
```

The library is synthetic and deterministic: the names come from a counter, not
from a random number generator, so the same work is measured on every
machine and every run.

## The numbers

| Operation | Cost | Budget |
| --- | --- | --- |
| Frame with a 100,000-track library | 0.23 ms (worst 0.29 ms) | 16 ms |
| Rebuild of the visible list (copy + sort) | 131 ms | 250 ms |
| Sort of 100,000 tracks by title | 73 ms | 250 ms |
| Copy of 100,000 tracks | 64 ms | 250 ms |
| A view that did not change | 0 ms | — |

A 60 fps frame is 16.7 ms. The frame itself costs well under a millisecond
whatever the library holds, because the songs table is **virtualised**: only
the rows on screen are built, so the library's size does not reach the
drawing code.

The rebuild is the one that costs real time, and it happens when the *view*
changes — a different tab, a different filter, a search. It is not a
per-frame cost: the state remembers what it last built and skips the work
when nothing that affects the list has changed.

## What was measured, and what was fixed

The evaluation found two costs that a 100,000-track library pays and a
1,000-track library does not notice.

### The sort re-ordered a copy of the library

Changing the sort column used to rebuild the whole visible list: the library
was copied out of the state, filtered and sorted again. The copy allocates a
new string for every field of every track — half a million allocations for
100,000 tracks — and then the sort moved two-hundred-byte records around.

**Fixed**: when only the sort changed, the list that is already on screen is
re-ordered in place. The library-budget test counts rebuilds
(`LibraryState::rebuilds`) and fails if a sort change starts rebuilding
again.

### The sort's own cost

The sort is a stable sort over cached keys: every key is computed once, so
the comparisons do not re-lower-case a title for each of them.

Two alternatives were measured and rejected:

- **A case-insensitive comparison that allocates nothing.** It looked like
  the better design — no allocation per key — but it was *five times slower*
  (635 ms against 73 ms): the cached keys are compared with a `memcmp`, while
  a comparator that walks the characters pays for every comparison.
- **Sorting an index permutation instead of the records.** The sort then
  moves four-byte indices rather than two-hundred-byte records, but putting
  the tracks back in order moves them anyway, and the extra pass made it
  slower than sorting the records directly (80 ms against 68 ms).

The measurements are in the git history of this file's first version; both
alternatives are recorded here so nobody tries them again.

## The budgets in CI

`crates/ui/tests/library_budget.rs` runs in the player CI job. It asserts:

- the rebuild of a 100,000-track library stays under 250 ms,
- the sort and the copy each stay under 250 ms,
- a frame of the songs table stays under 16 ms,
- an unchanged view is not rebuilt,
- a sort change does not rebuild the list.

The budgets are deliberately loose multiples of the measured times. A machine
slower than the one the numbers were measured on must not fail the build;
what the budget catches is a change that makes the rebuild an order of
magnitude slower — a clone that became a deep copy, a sort key that started
allocating per comparison.

## What is not measured

- **Decoding and the audio thread.** The engine has its own benchmarks
  (`crates/engine/benches`), and its frame budget is a different question.
- **The GPU.** The numbers above cover the UI thread's work: the layout and
  the widget building. The painting itself is the graphics driver's, and it
  does not grow with the library.
- **A cold start.** Opening a library is a different measurement — it is
  dominated by the database read, not by the list.
