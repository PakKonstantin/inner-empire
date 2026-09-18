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

Most of the bridge is testable on any platform:

```sh
cargo test -p ie-ffi                 # bridge, error mapping, record mirrors
cargo test -p ie-core                # unchanged, must stay green
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

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
