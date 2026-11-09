# Changelog

What changed in each release. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Fewer rank lookups. Filtering by rank only looks up opponents from the games your other filters keep, skips games from before ranked began, and fetches a finished season once, so a new season no longer refetches everyone. Each of those lookups also refreshes the player's current rank.
- Opponents you haven't played in a month have their rank refreshed weekly instead of daily.
- Scrolling quickly no longer queues lookups for rows you've scrolled past; the rows on screen go first.

## [1.0.0] - 2026-11-09

The first release of the rewrite: a new app, not an update to the old one.

### Added

- **Tonight**: your session so far, how far your rating moved, and the opponent you're playing right now, with a desktop notification when a game starts.
- **Replays**: every game, newest first. Ranked sets, and unranked sessions with the same person, fold into one row with the score. Filter by date, ranked season, mode, character, stage and your opponent's rank in that season.
- **Search** by netplay name, connect code or in-game tag.
- **Stats** for each game (Overview, Kills, Punishes, Actions), worked out the way slippi-js does it.
- **Play** a game, a set, a kill or a punish in Slippi's playback Dolphin.
- **Opponents**: your record against everyone, by their character and by stage, with notes.
- **Me**: your record, ranks and seasons, your numbers next to your opponents', week-by-week trends and a matchup report.
- **Highlights**: search every punish by damage, kills or zero-to-deaths and watch them back to back.
- More than one replay folder, nested folders, and replays copied into several places counted once.
- Downloads for Windows, macOS (Apple Silicon and Intel) and Linux on the Releases page.

### Changed

- Licensed under GPL-3.0-or-later (the stats crate, a port of slippi-js, under LGPL-3.0-or-later). Earlier versions were MIT or Apache-2.0.

[Unreleased]: https://github.com/rj-jones/eppi/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/rj-jones/eppi/releases/tag/v1.0.0
