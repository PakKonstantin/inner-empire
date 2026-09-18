//! Generate the performance test vault.
//!
//! Committed as a generator rather than as a thousand files: those would bloat
//! the repository, diff badly, and go stale silently. This is deterministic
//! from a seed, so "the test vault" means the same thing on every machine, and
//! it prints a manifest hash so a test can assert that.
//!
//! ```sh
//! cargo run -p ie-ffi --example gen-test-vault -- /tmp/big-vault
//! cargo run -p ie-ffi --example gen-test-vault -- /tmp/big-vault --notes 5000
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A small deterministic PRNG. Not for anything that matters; what matters here
/// is that two machines produce the same vault.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// A long tail: most notes have a few links, a handful have many. Uniform
    /// degree would make the graph and the backlinks panel far easier than they
    /// are in a real vault.
    fn degree(&mut self) -> usize {
        match self.below(100) {
            0..=4 => 20 + self.below(30),
            5..=24 => 8 + self.below(8),
            _ => 1 + self.below(5),
        }
    }
}

const FOLDERS: &[&str] = &[
    "Notes",
    "Notes/Reading",
    "Notes/Meetings",
    "Projects",
    "Projects/Archive",
    "Daily",
    "Reference",
    "Reference/Deep/Nesting/Here",
];

const TAGS: &[&str] = &[
    "project",
    "project/alpha",
    "project/beta",
    "project/alpha/ui",
    "reading",
    "reading/paper",
    "reading/book",
    "meeting",
    "meeting/weekly",
    "idea",
    "idea/half-baked",
    "reference",
    "todo",
    "todo/urgent",
    "done",
    "person",
    "person/team",
    "place",
    "quote",
    "recipe",
];

const WORDS: &[&str] = &[
    "system",
    "boundary",
    "index",
    "vault",
    "surface",
    "pattern",
    "protocol",
    "cache",
    "gradient",
    "lattice",
    "threshold",
    "signal",
    "archive",
    "fragment",
    "interval",
    "structure",
    "margin",
    "trace",
    "contour",
    "register",
    "corpus",
    "schema",
    "anchor",
    "harbour",
    "meridian",
    "compass",
    "ledger",
    "atlas",
    "beacon",
    "cipher",
];

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        eprintln!("usage: gen-test-vault <dir> [--notes N] [--seed N]");
        std::process::exit(2);
    };
    let root = PathBuf::from(root);

    let mut note_count = 1000usize;
    let mut seed = 0x1E_1E_1E_1Eu64;
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_default();
        match flag.as_str() {
            "--notes" => note_count = value.parse().expect("--notes takes a number"),
            "--seed" => seed = value.parse().expect("--seed takes a number"),
            other => {
                eprintln!("unknown option {other}");
                std::process::exit(2);
            }
        }
    }

    if root.exists() {
        eprintln!(
            "{} already exists; refusing to write into it",
            root.display()
        );
        std::process::exit(1);
    }

    let mut rng = Rng(seed);
    let titles: Vec<String> = (0..note_count)
        .map(|n| {
            let a = WORDS[(n * 7) % WORDS.len()];
            let b = WORDS[(n * 13 + 5) % WORDS.len()];
            // Title Case, and deliberately not unique-by-construction: several
            // notes share a stem so link resolution has real ambiguity to
            // resolve.
            format!(
                "{}{} {}{} {n}",
                a[..1].to_uppercase(),
                &a[1..],
                b[..1].to_uppercase(),
                &b[1..]
            )
        })
        .collect();

    let paths: Vec<String> = (0..note_count)
        .map(|n| format!("{}/{}.md", FOLDERS[n % FOLDERS.len()], titles[n]))
        .collect();

    for folder in FOLDERS {
        std::fs::create_dir_all(root.join(folder)).unwrap();
    }
    std::fs::create_dir_all(root.join("Attachments")).unwrap();

    let mut manifest: BTreeMap<String, String> = BTreeMap::new();

    for n in 0..note_count {
        let mut body = String::with_capacity(1024);

        body.push_str("---\n");
        body.push_str(&format!("title: {}\n", titles[n]));
        body.push_str("tags:\n");
        for _ in 0..(1 + rng.below(4)) {
            body.push_str(&format!("  - {}\n", TAGS[rng.below(TAGS.len())]));
        }
        body.push_str(&format!(
            "created: 2026-{:02}-{:02}\n",
            1 + n % 12,
            1 + n % 28
        ));
        body.push_str(&format!("wordCount: {}\n", 80 + rng.below(400)));
        body.push_str(&format!("starred: {}\n", n % 7 == 0));
        body.push_str(&format!("rating: {}.{}\n", rng.below(5), rng.below(10)));
        if n % 5 == 0 {
            body.push_str("author:\n  name: A Person\n  role: writer\n");
        }
        body.push_str("---\n\n");

        body.push_str(&format!("# {}\n\n", titles[n]));

        for section in 0..(1 + rng.below(4)) {
            body.push_str(&format!("## {}\n\n", WORDS[rng.below(WORDS.len())]));
            for _ in 0..(2 + rng.below(4)) {
                let mut line = String::new();
                for _ in 0..(6 + rng.below(14)) {
                    line.push_str(WORDS[rng.below(WORDS.len())]);
                    line.push(' ');
                }
                body.push_str(line.trim_end());
                body.push_str(".\n");
            }
            body.push('\n');
            if section == 0 {
                body.push_str(&format!("A referenceable line. ^block-{n}\n\n"));
            }
        }

        // Links, at a realistic degree distribution.
        body.push_str("## See also\n\n");
        for _ in 0..rng.degree() {
            let target = rng.below(note_count);
            body.push_str(&match rng.below(10) {
                0 => format!("- Embedded: ![[{}]]\n", titles[target]),
                1 => format!("- Aliased: [[{}|something else]]\n", titles[target]),
                2 => format!(
                    "- A heading: [[{}#{}]]\n",
                    titles[target],
                    WORDS[rng.below(WORDS.len())]
                ),
                3 => format!("- A block: [[{}#^block-{target}]]\n", titles[target]),
                4 => format!("- Markdown: [{}](<{}>)\n", titles[target], paths[target]),
                _ => format!("- [[{}]]\n", titles[target]),
            });
        }
        // Some links that resolve to nothing: an index that only ever sees
        // resolvable links is not being tested.
        if n % 11 == 0 {
            body.push_str(&format!("- Unresolved: [[Not A Note {n}]]\n"));
        }
        body.push('\n');

        body.push_str(&format!(
            "Inline tags: #{} and #{}.\n",
            TAGS[rng.below(TAGS.len())],
            TAGS[rng.below(TAGS.len())]
        ));

        // Code, so the parser has something it must *not* index.
        if n % 9 == 0 {
            body.push_str("\n```rust\n// [[not a link]] and #not-a-tag\nfn main() {}\n```\n");
        }

        if n % 23 == 0 {
            body.push_str("\n![[diagram.png]]\n");
        }

        write(&root.join(&paths[n]), body.as_bytes(), &mut manifest, &root);
    }

    // Two names that differ only by case, so the collision diagnostics have
    // something real to report on a case-sensitive volume.
    write(
        &root.join("Notes/Case Study.md"),
        b"# Case Study\n\nUpper.\n",
        &mut manifest,
        &root,
    );
    write(
        &root.join("Notes/case study.md"),
        b"# case study\n\nLower.\n",
        &mut manifest,
        &root,
    );

    for n in 0..40 {
        let name = format!("Attachments/figure-{n:02}.png");
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend((0..512).map(|i| ((i * 31 + n * 7) % 251) as u8));
        write(&root.join(&name), &bytes, &mut manifest, &root);
    }

    write(
        &root.join("Board.canvas"),
        br#"{"nodes":[],"edges":[]}"#,
        &mut manifest,
        &root,
    );

    let digest = hash(&manifest);
    println!("{} notes written to {}", note_count + 2, root.display());
    println!("{} files, manifest {digest}", manifest.len());
}

fn write(path: &Path, bytes: &[u8], manifest: &mut BTreeMap<String, String>, root: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
    let relative = path
        .strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    manifest.insert(relative, format!("{}", bytes.len()));
}

/// A stable digest of the manifest, so "the same vault" is checkable.
///
/// FNV-1a rather than a real hash: this identifies a generator run, it does not
/// defend against anything.
fn hash(manifest: &BTreeMap<String, String>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (path, size) in manifest {
        for byte in path.bytes().chain(b":".iter().copied()).chain(size.bytes()) {
            h ^= byte as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    format!("{h:016x}")
}
