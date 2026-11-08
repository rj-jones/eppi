# Eppi

A fast desktop app for your [Slippi](https://slippi.gg/) replays: sets, stats, ranks and the people you play.

![Eppi's replay list](docs/screenshots/2-replays.png)

Eppi reads every replay in your Slippi folder and keeps an index, so it opens in a second even with thousands of games. It watches the folder while you play. The moment a game starts, it shows who you're playing, their rank and your record against them.

## What it does

- **Tonight.** The session you're in: sets and games won and lost, how far your rating moved, and the opponent you're playing right now. You also get a desktop notification when a game starts while Eppi is in the background.
- **Replays.** Every game, newest first. Ranked sets fold into one row with their score (2–1), and so do unranked sessions with the same person. Each row shows the opponent's rank. You can filter by mode, character, stage, date or ranked season, and by the rank your opponent had in the season you played them. You can also hide short and CPU games, and search by name, code, character or stage.
- **Stats.** Each game's stats as Slippi Launcher shows them (Overview, Kills, Punishes, Actions). The numbers are computed the way slippi-js computes them and checked against it on real replays. Every kill and punish can be played from that moment.
- **Play.** Play a game, a whole set, a kill or a punish in Slippi's playback Dolphin, found automatically from Slippi Launcher.
- **Opponents.** Everyone you've played, with your record against them and their current rank. For each one: how you do against each of their characters and on each stage, your history, and your notes.
- **Me.** Your record, your rank and its history (including how you finished every past season), and your numbers against your opponents': openings per kill, damage per opening, neutral wins, L-cancels and more. Also your results by matchup, stage and character, a week-by-week table to see whether you're improving, and a matchup report (what kills you, what you kill with, which openings pay off).
- **Highlights.** Search every punish you've ever landed (or taken) by damage, kills or zero-to-deaths, and watch the best ones back to back.

Everything works from the keyboard: arrows move, Enter opens, Space plays, `/` searches, Esc goes back, Ctrl+1–5 switch views and F5 looks for new replays.

## Install (Windows)

```powershell
pwsh -File scripts\install.ps1              # builds, copies to %LOCALAPPDATA%\Programs\Eppi, adds a Start menu entry
pwsh -File scripts\install.ps1 -Uninstall   # removes both
```

Or run it from the repo with `cargo run --release`.

The first time it starts, Eppi takes your connect code and replay folder from Slippi Launcher. Change them in Settings: add alt codes so those games count as yours, and add every folder that holds replays. Folders inside each other are read once, and a replay copied into several places counts once. The index lives in `%LOCALAPPDATA%\Eppi`.

It also builds on macOS and Linux, but it's only tested on Windows.

## How it's built

- `src/`: the app (egui with blabbur's Melange theme). `library/` keeps the SQLite index in step with the replay folders on a worker thread. The first pass reads players and results without frames, so 8,600 replays index in seconds. The second pass computes full stats in the background, newest games first. `ui/` holds the views; the lists use blabbur's virtual list, so only the rows on screen are drawn.
- `crates/eppi-stats`: slippi-js's stats ported to Rust on [peppi](https://github.com/hohav/peppi). `tools/verify-stats` compares its output with slippi-js on real replays.
- `crates/eppi-rank`: rank lookups by connect code, using the API and the rank tiers Slippi Launcher uses. See its `RESEARCH.md`.
- `scripts/install.ps1`: the Windows install.

```sh
cargo test --workspace
cargo run --release -- --screenshots docs/screenshots   # captures each view, then quits
```

## Credits

The character, stage and rank art comes from [Slippi Launcher](https://github.com/project-slippi/slippi-launcher). Replays are read with [peppi](https://github.com/hohav/peppi), and the stats follow [slippi-js](https://github.com/project-slippi/slippi-js). Icons are [Ionicons](https://ionic.io/ionicons) (MIT), and the heading font is Ubuntu (Ubuntu Font Licence).

Licensed under MIT or Apache-2.0.
