# AutoEQ parametric corpus

`autoeq_parametric.bin` is **generated and pinned**, not compiled at build time.

- Generator: `scripts/generate-autoeq-corpus.py` (documented in its module
  docstring: source data, filter-id mapping, preamp rule, fixed-width layout).
- Why pinned: the UI decodes the file directly at runtime with a single
  allocation and no parsing; regenerating it during a build would make the
  shipped tuning depend on the machine that built it and on the AutoEQ source
  database being reachable.
- To update: run the script, then commit the new `.bin` together with the
  `ATTRIBUTION.md` change if the upstream lineage moved.

Licensing for the curves is in `LICENSE-AutoEq` and `ATTRIBUTION.md`.
