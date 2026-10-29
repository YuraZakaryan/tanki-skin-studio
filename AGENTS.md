# AGENTS.md — rules for every contributor and agent

Tanki Skin Studio is small, so the process is light. A few rules are absolute because what they
protect cannot be undone: a file in a public commit stays in its history, and a user's game cache is
theirs.

## 1. Provenance (absolute)

- **Never commit game content:** no textures, models, previews, sounds, `.tara`/`.swf` files, cache
  folders or dumps, skin packs or archives, not even in a test, and not "just for a moment".
- **Tests use synthetic data only.** Anything a test needs is generated at run time by
  `crates/tanki-testkit`. Tests against a real cache stay opt-in (`apps/tanki-studio/tests/real.rs`).
- **Screenshots** in `docs/images/` show the app on the synthetic cache only (`cargo xtask e2e-ui`).
- **Never name a game, a game client, its loader or its developer** anywhere: code, identifiers,
  paths, tests, docs, interface text, the design file, commit messages, issues, the GitHub page.
  The app finds the client on the user's machine instead: the cache sits in the standard Adobe AIR
  application storage (`<application id>/Local Store/cache`), and the running client is the program
  whose AIR descriptor (`META-INF/AIR/application.xml`) declares that id. Any name the user sees
  comes from their own machine. In code the cache format is `air-cache`.
- **Fonts and icons** come under open licences with their licence file next to them in
  `crates/tanki-ui/assets/`. If an asset's origin or licence is unclear, do not add it.
- `cargo xtask assets` (part of `cargo xtask ci`) refuses game file types and names, binaries outside
  their folders, files over 4 MiB and those names. Never weaken it to make a change pass.

## 2. Never crash, never lose a file

- Cache files, packs, archives, settings and keystrokes are untrusted input. No new `unwrap`,
  `expect`, `panic!` or `unreachable!` outside tests unless provably infallible, with a comment saying
  why. Use `get` instead of indexing, slice strings only at char boundaries, use checked or saturating
  arithmetic on numbers from input, and cap sizes. No `unsafe`.
- Every write to a game cache goes through a `tanki-apply` transaction: backed up and verified first,
  rolled back on failure, restorable byte for byte. Nothing else writes to the cache.
- No network access and no telemetry. Adding either is the owner's decision.

## 3. Architecture

- Crates are layered; a crate depends only on lower layers (`cargo xtask layers`,
  [docs/architecture.md](docs/architecture.md)). The pattern follows PdfCraft.
- The app is a library (`tanki_studio`): state in `app.rs`, every user action a method in
  `actions.rs`, one file per screen in `views/`, widgets from `tanki-ui`.
- Interface strings are English keys (`tl!("…")`) with a Russian row in
  `crates/tanki-i18n/src/ru.tsv`; `cargo xtask i18n --template` prints the missing rows.
- For the Garage, the app is the source of truth; `design.pen` follows it.

## 4. Before every commit

- `cargo xtask ci` is green: provenance, formatting, translations, Clippy with `-D warnings`,
  layering, all tests.
- For interface changes, `cargo xtask e2e-ui` too (it needs a desktop session).
- A bug fix comes with a test that fails without the fix: revert the fix once and watch it go red.
- Update [docs/test-checklist.md](docs/test-checklist.md) when a behaviour gains or changes a test.

## 5. Branches, commits and pull requests

- `main` is the only long-lived branch and must always be green.
- The maintainer may commit small, green changes straight to `main`.
- Everyone else, and any larger or riskier change, goes through a pull request:
  - branch from an up-to-date `main` as `<type>/<slug>` (`feat/`, `fix/`, `docs/`, `chore/`,
    `refactor/`);
  - fill in [the pull request template](.github/pull_request_template.md);
  - merge only with green CI, as a squash merge;
  - rebase your own branch when `main` moves; never rewrite someone else's.
- Commit messages: an imperative subject of at most 72 characters, then a body saying why. Work
  written with an AI model carries a `Co-Authored-By:` trailer naming it.
- Never commit secrets, local paths or machine-specific files (`/target`, `/dist`,
  `/graphify-out` are ignored).

## 6. When outside contributors arrive

Turn on what a bigger project needs, in this order: branch protection on `main` (pull request and
green CI required), an automatic reviewer such as CodeRabbit, and `cargo-deny` for dependency
licences and sources.
