# iOS development

## 1. What you need

| | Version | Why |
|---|---|---|
| macOS | 14+ | Xcode 16 requirement |
| Xcode | 16+ | SwiftUI `@Observable`, Swift 6 concurrency |
| Rust | 1.80+ | workspace `rust-version` |
| XcodeGen | 2.4+ | generates the `.xcodeproj` from `project.yml` |

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
cargo install uniffi-bindgen-cli --version 0.32
brew install xcodegen
```

## 2. Building

```sh
./ios/Scripts/build-core.sh          # cargo → lipo → xcframework → bindings
cd ios && xcodegen generate          # project.yml → InnerEmpire.xcodeproj
open InnerEmpire.xcodeproj
```

`build-core.sh` does, in order:

1. `cargo build -p ie-ffi --release --target <each iOS target>`
2. `lipo` the two simulator slices into one fat library
3. `xcodebuild -create-xcframework` → `ios/Frameworks/InnerEmpireCore.xcframework`
4. `uniffi-bindgen generate --library … --language swift` →
   `ios/Generated/InnerEmpireCore.swift` + module map

Step 4 also runs on Linux, which is how the bindings are kept in review without
a Mac. They are checked in; CI regenerates and fails on a diff, so the committed
Swift can never lag the Rust.

## 2a. Design tokens

```sh
node ios/Scripts/generate-tokens.mjs
```

Reads `src/styles/theme.css` and writes `ios/Services/DesignTokens.swift`: 42
colours and 15 dimensions, with the light and dark values the desktop theme
already defines. Change a colour in the CSS and regenerate; CI fails if the
committed Swift is stale, which is what stops "the accent colour" becoming two
values that drift.

Only colours and dimensions cross. Fonts and shadows stay per-platform, because
iOS has Dynamic Type and system materials and forcing the desktop's answers
would be worse than not sharing at all.

## 3. Signing

Never in Git. `ios/Config/Signing.xcconfig` is `.gitignore`d;
`Signing.xcconfig.example` is committed:

```
IE_DEVELOPMENT_TEAM = ABCDE12345
IE_BUNDLE_ID_PREFIX = com.example
```

`project.yml` reads `DEVELOPMENT_TEAM = $(IE_DEVELOPMENT_TEAM)` and builds every
bundle identifier from the prefix, so a fork needs one untracked file and no
source edits.

## 4. Why the project file is generated

A `.pbxproj` is a 3000-line plist with UUID cross-references. It cannot be
reviewed in a diff and it conflicts on every parallel branch. `project.yml` is
80 lines of YAML that says the same thing, and it is the artifact that gets
reviewed. Run `xcodegen generate` after pulling.

## 5. Working on the Rust side without a Mac

Run everything CI runs, in the order CI runs it:

```sh
pnpm check:all
```

That is the one to use before pushing. It exists because running the pieces
by hand is how a late fix gets pushed unformatted: the format check comes
first, as it does in CI, so a change made after the last `cargo fmt` cannot
slip past.

The pieces, when you want one of them:

```sh
pnpm rust:fmt:check                  # formatting, first for the reason above
pnpm rust:clippy                     # the whole workspace, deny warnings
pnpm rust:test:ios                   # core and platform without desktop backends
pnpm rust:test                       # 515 tests
pnpm ios:generated                   # bindings and tokens match their sources
pnpm ios:symbols                     # names referenced from Swift resolve
pnpm ios:appgroup                    # app and extension share an App Group
pnpm ios:docs                        # the docs name things that exist
pnpm ios:a11y                        # unlabelled icon buttons, fixed font sizes
pnpm ios:bindings                    # regenerate the Swift bindings
pnpm ios:tokens                      # regenerate the design tokens
```

`pnpm ios:generated` generates into a temporary directory and compares, rather
than regenerating in place and asking git. Asking git cannot work before a
commit: the files you just regenerated are exactly the uncommitted changes it
would complain about, so the check could only ever pass after the commit it was
meant to gate.

`pnpm ios:symbols` is not a type check and does not pretend to be. There is no
Swift compiler here, so it verifies the one class of error that is mechanical:
a name referenced that does not exist.

What it covers, and how:

| | Derived from | Gap |
|---|---|---|
| Design tokens | the generated file plus the semantic aliases | none |
| `VaultService` members | the service's own source | none |
| Bridge types | a declared list, checked against the bindings | a new type used but not listed |
| Bridge functions | a declared list, checked both ways | a new call added but not listed |
| `VaultHandle` methods | the generated protocol | none, for calls on `handle` |
| Record *fields* | not checked | `results.files` for `results.hits` |

The two declared lists are declared on purpose. Telling a function call from an
enum case or a closure invocation needs a Swift parser; a regex that guesses
reports forty false positives on this tree, and a check that cries wolf gets
turned off. So the app states which bridge symbols it depends on, and the
script verifies both that the bindings export each one *and* that something
actually calls it — so the list cannot quietly become a list of names nobody
uses.

Field access on a bridge record is the one gap worth naming, because a real
bug lived there: `SearchResults` has `hits`, and `results.files` was written
instead. A check for it was measured rather than assumed — of nine candidate
accesses in this tree, nine were ordinary Swift members (`.map`, `.isEmpty`,
`.first`). A check with that false-positive rate gets turned off, so the gap is
documented instead. A one-off audit found exactly one instance, now fixed.

Everything a compiler would catch — types, generics, protocol conformance,
actor isolation, exhaustive switches — still waits for a Mac.

The iOS `FileSystem` adapter is tested on the host target against a temp
directory, with the two callback hooks stubbed — the POSIX paths are identical,
and what the stubs verify is that the hooks are called on exactly the right
conditions and no others.

Cross-compiling to `aarch64-apple-ios` needs the iOS SDK and therefore a Mac;
`libsqlite3-sys` bundles C.

## 6. Layout

```
ios/
  project.yml                 XcodeGen spec — edit this, not the xcodeproj
  Config/                     xcconfig per configuration
  Frameworks/                 built xcframework (gitignored)
  Generated/                  UniFFI Swift bindings (committed)
  App/                        entry point, scene delegate, lifecycle
  Features/
    Notes/  Search/  Graph/  Settings/  Sync/  Capture/
  Editor/                     UITextView bridge, accessory bar, syntax layer
  Services/                   VaultService actor, indexing, bridge wrappers
  Storage/                    VaultStorage, bookmarks, coordination, hooks
  Extensions/
    Share/  Widgets/  Intents/
  Resources/                  assets, Info.plist, entitlements
  Scripts/build-core.sh
  Tests/{Unit,UI}/
```

## 7. Conventions

- Swift 6 language mode, complete concurrency checking on.
- No `@MainActor` on anything that touches the filesystem.
- View models are `@Observable` classes, `@MainActor`-isolated, with no
  reference to `VaultHandle` — only to `VaultService`.
- Every user-facing string goes through `String(localized:)` from the start,
  even though only English ships, because retrofitting is worse.
- No force unwraps outside tests.
