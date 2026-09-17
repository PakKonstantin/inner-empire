# Markdown conformance corpus

Two implementations parse the app's Markdown extensions: `ie-core::markdown`
in Rust, which is authoritative and feeds the index, and `src/markdown` in
TypeScript, which renders and decorates.

Two implementations of one grammar drift. These fixtures are what stops them:
each `*.md` file has a matching `*.expected.json`, and **both** test suites
read the same corpus and must produce the same answer.

- Rust: `cargo test -p ie-core --test conformance`
- TypeScript: `pnpm test src/markdown/conformance.test.ts`

Adding a construct means adding a fixture here first. A change implemented in
only one language fails in the other, which is the point.

The expected shape covers the *semantic* surface the two must agree on — what
is a link, a tag, a heading, a block — not byte offsets, which each side is
free to represent as it likes.
