# Security

If you find a security problem in Eppi, please report it privately. Don't open a public issue. Use **[Report a vulnerability](https://github.com/rj-jones/eppi/security/advisories/new)** on the Security tab. I'll reply as soon as I can, and once it's fixed I'll credit you in the release notes unless you'd rather I didn't.

Only the latest release gets fixes.

## What Eppi touches

So you know where to look:

- It reads `.slp` replay files from the folders you pick, and keeps an index in a local SQLite database.
- It sends connect codes to Slippi's API (`https://internal.slippi.gg/graphql`) to look up ranks. Nothing else leaves your computer, and Settings can turn the lookups off.
- It starts Slippi's playback Dolphin with a small JSON file that says which replay to play.
- It reads Slippi Launcher's settings to find your connect code, replay folder and Dolphin.
