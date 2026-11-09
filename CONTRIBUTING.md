# Contributing

Bug reports, ideas and pull requests are welcome. For anything bigger than a fix, open an issue first so we can agree on the approach before you put time in.

## Building

You need Rust 1.95 or newer ([rustup](https://rustup.rs/)). On Linux you also need the headers eframe's window uses:

```sh
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libdbus-1-dev
```

Then:

```sh
cargo run --release              # the app, with your own replays
cargo run --release -- --showcase target/shots   # the screenshot tour on demo data, then quits
cargo test --workspace
```

## Before you open a pull request

CI runs these and fails on any warning, so run them first:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

- Keep a pull request to one change, and say in it what you changed and how you checked it. For UI changes, add a screenshot.
- Changes to `crates/eppi-stats` have to keep matching slippi-js. `tools/verify-stats` runs both on a folder of replays and compares them; the comments at the top of its scripts say how to run it.
- If your change is something users will notice, add a line under **Unreleased** in `CHANGELOG.md`.
- Don't commit replays or screenshots that show other players' names or codes without their say-so. `--showcase` replaces them with made-up ones.

## Licence

Eppi is GPL-3.0-or-later, and `crates/eppi-stats` is LGPL-3.0-or-later. By sending a pull request you agree your change is licensed the same way as the code it touches.

## Releases

For maintainers: bump `version` in `Cargo.toml` (and the crates' if they changed), move the **Unreleased** notes in `CHANGELOG.md` under the new version, commit, then push a tag:

```sh
git tag v1.2.3 && git push origin v1.2.3
```

`.github/workflows/release.yml` builds the Windows, macOS and Linux downloads and publishes the release with that version's changelog notes.
