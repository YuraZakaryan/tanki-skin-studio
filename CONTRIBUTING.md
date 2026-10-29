# Contributing

Thanks for helping. The rules every change follows, for people and AI agents alike, are in
**[AGENTS.md](AGENTS.md)**. This page is the practical side.

## Before you start

- Look for an existing issue or pull request first, open or closed.
- For anything bigger than a fix, open an issue to agree on the approach.
- Security problems go to the private channel in [SECURITY.md](SECURITY.md), never to an issue.

## Set up

You need Rust 1.95 or later (the Xcode Command Line Tools on macOS, the MSVC toolchain on Windows).

```sh
git clone https://github.com/YuraZakaryan/tanki-skin-studio.git
cd tanki-skin-studio
cargo run -p tanki-studio
```

No game is needed: every test runs on a synthetic cache that `crates/tanki-testkit` generates.

## Make the change

1. Branch from an up-to-date `main`: `feat/…`, `fix/…`, `docs/…`, `chore/…` or `refactor/…`.
2. Add tests with the change. A bug fix comes with a test you watched fail without the fix.
3. Run the gate: `cargo xtask ci`, and `cargo xtask e2e-ui` for interface changes (it needs a
   desktop session). `cargo deny check` checks the dependency policy if you changed dependencies.
4. Open a pull request and fill in the template.

## Never commit

- game content of any kind: textures, models, previews, sounds, cache files, packs, archives;
- the name of any game, game client, loader or developer;
- secrets, personal paths, build output.

`cargo xtask assets` refuses most of this automatically.

## Licence of contributions

The project is licensed under GPL-3.0-or-later ([LICENSE](LICENSE)). By opening a pull request you
agree that your contribution is licensed the same way.

## Conduct

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
