<!--
Thanks for contributing to Tanki Skin Studio. Base this on an up-to-date `main` and read
AGENTS.md first. Delete any line that does not apply.
-->

## What this changes

<!-- What is different afterwards, and why it is worth doing. `Fixes #123` if it fixes an issue. -->

## How you verified it

<!-- The commands you ran and what they said. -->

- [ ] `cargo xtask ci` is green
- [ ] `cargo xtask e2e-ui` is green (interface changes only)
- [ ] New behaviour has a test; a bug fix has a test I watched fail without the fix
- [ ] `docs/test-checklist.md` and the docs are updated if behaviour changed

## Provenance

This cannot be undone after a merge: a file in a public commit stays in its history.

- [ ] No game content: no textures, models, previews, sounds, cache files, packs or archives,
      not even in a test (`tanki-testkit` generates what tests need)
- [ ] No game, game client, loader or developer is named anywhere; the app finds the client on the
      user's machine
- [ ] Any new font or icon has an open licence, with its licence file next to it

## Safety

- [ ] No new `unwrap`/`expect`/`panic!` outside tests, no indexing with positions from input,
      no `unsafe`
- [ ] Every write to the game cache goes through a `tanki-apply` transaction and can be restored
- [ ] No network access or telemetry added

## Anything a reviewer should look at first

<!-- The judgement call you are least sure about, or a change existing users will notice. -->
