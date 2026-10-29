# Contributing

1. `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` must pass (CI runs them).
2. **No panics in library code**: no `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, and no unchecked
   indexing on data that comes from files, parameters or users. Return `Result`/`Option`; degenerate geometry
   returns empty output, never a crash. Tests may unwrap.
3. **Clean-room.** Write from public specifications and your own test files. Do not paste code from commercial
   embroidery software or from GPL/AGPL projects (e.g. Ink/Stitch). Permissively licensed code (e.g. pyembroidery,
   MIT) may be used as a *reference for checking* output; if you copy any, keep its licence notice and record it
   in `NOTICE`.
4. User-visible text must not name other vendors' products; documentation and comments may.
5. Stitch generators must be deterministic and come with tests for: stitch-length limits, staying inside the
   shape, holes, degenerate input.
6. Machine-file changes need a round-trip test **and** a cross-check with an independent decoder
   (`tools/crosscheck_dst.py`). Anything sewn on real hardware: note machine, format, result in
   `docs/MACHINE_TESTING.md`.
