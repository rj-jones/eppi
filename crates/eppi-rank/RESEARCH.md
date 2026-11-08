# Slippi rank lookup: research notes (2026-11-08)

How Eppi can fetch players' ranks, and what was tried. 19 live requests in total, a few seconds apart, all with the user agent `Eppi/1.0 (+https://github.com/rj-jones/eppi)`.

## Short version

- There is no better source than the endpoint the old Eppi used. `https://internal.slippi.gg/graphql` is the URL compiled into the official launcher and into Slippi Dolphin.
- The better way is to batch. One request can look up many connect codes using GraphQL aliases (tested up to 150), and it can return wins, losses, continent, characters and season in the same request.
- Nothing local stores opponents' ranks. Dolphin and the launcher only ever fetch the player's own rank.
- Recommendation: aliased batches of 25 codes, run one after another, with each code's result cached for the session (or ~10 minutes). The crate does the batching. Caching and pacing are the app's job (see "For the integrator").

## Endpoint

- The launcher source reads `process.env.SLIPPI_GRAPHQL_ENDPOINT` (`slippi.service.ts`; `.env.example` has only `example`; CI injects the real value from secrets).
- The installed launcher's `app.asar` has the URL baked in: `https://internal.slippi.gg/graphql`, the only GraphQL URL in it.
- Slippi Dolphin's `slippi_rust_extensions.dll` uses the same URL. It also has `https://users-rest-dot-slippi.uc.r.appspot.com/user`, but that endpoint fetches a user by Firebase UID, which is useless for looking up opponents.
- The launcher sends a Firebase bearer token when logged in, but `getUser(connectCode:)` works without one. All of our requests were anonymous.

## Introspection

Disabled. `__type` queries return HTTP 400 with `INTROSPECTION_DISABLED` (Apollo Server behind Express on Google Frontend).

Field suggestions are still on, though. Querying a misspelled field returns `Did you mean "..."?`, and that is how these fields were found and then confirmed with real queries:

| Type | Fields confirmed |
|---|---|
| `User` | `displayName`, `connectCode { code }`, `status` (seen: `ACTIVE`), `fbUid`, `rankedNetplayProfile`, `rankedNetplayProfileHistory`, `unrankedRating { ordinal mu sigma }` (null for the test account), `activeSubscription`, `allSubscriptions` |
| `NetplayProfile` | `id`, `ratingOrdinal`, `ratingUpdateCount`, `wins`, `losses`, `ties`, `dailyGlobalPlacement`, `dailyRegionalPlacement`, `continent` (e.g. `NORTH_AMERICA`, `EUROPE`), `characters { character gameCount }`, `season { id name status startedAt endedAt }`, `ratingMu`, `ratingSigma`, `mmr`, `user` |

Notes:
- `rankedNetplayProfileHistory` returns one profile per past season (Seasons 2 to 5 for `BEAN#888`), each with its own rating, sets, W/L, characters and season dates. That is enough for a rank history view.
- `NetplayProfile.id` is `RANKED_SINGLES-<firebase uid>-season-N`, so it leaks the account UID. The crate doesn't ask for it.
- `fbUid` is also queryable by connect code. Same concern; not used.
- Every existing account came back with a `rankedNetplayProfile`. Accounts that haven't played this season get rating 1100, 0 sets, and an empty character list, which the launcher's rules make rank None. A `null` profile was never seen, but the crate still handles it.
- No field marks a ban. `status` is the obvious candidate, but only `ACTIVE` was observed. The crate treats `status == "BANNED"` as `Rank::Banned`, which is a guess.

## Batching with aliases

`query($c0: String, $c1: String, ...) { u0: getUser(connectCode: $c0) {...} u1: ... }` works.

| Codes per request | HTTP | Time | Body |
|---|---|---|---|
| 1 | 200 | ~0.15 s | |
| 4 | 200 | 0.24 s | |
| 60 | 200 | 1.34 s | 15.5 KB |
| 150 | 200 | 3.21 s | 39 KB |

The time grows roughly linearly, about 20 ms per code, so the server seems to resolve the aliases one at a time. Batching saves round trips, not server work.

Other behavior:
- Unknown or malformed codes (`ZZZZ#99999`, `NOHASH`, `""`) return `null` for that alias, not an error. The rest of the batch still resolves.
- Lookups are case-sensitive. `bean#888` returns `null`; `BEAN#888` works. Replays store the full-width `＃`. The crate normalizes both.
- Validation errors (unknown field, missing sub-selection) fail the whole request with HTTP 400 and an `errors` array.

## Rate limiting

None observed. Eight back-to-back requests (0.3 s apart) all returned 200 in about 0.16 s each. No `RateLimit-*`, `X-RateLimit-*` or `Retry-After` headers appeared. Response headers are `cache-control: no-store`, an `etag`, and `x-cloud-trace-context`. Throttling may well exist at a higher volume, so we did not push further.

## Local sources

- Launcher's `%APPDATA%\Slippi Launcher\slippi.sqlite` (opened read-only): tables `file`, `game`, `player` and migration tables. `player` has `connect_code`, `display_name`, `tag`, `user_id`, `character_id` and similar, but no rank or rating.
- Dolphin's `netplay\User\Slippi\user.json` holds the logged-in user's own identity only. `direct-codes.json` and `teams-codes.json` hold recently used codes and `lastPlayed`.
- Dolphin's rank code (`rank_fetcher` in the Rust extensions) runs `getRankedMatchPersonalResult(request: OnlineMatchRequestInput!)`. After a ranked match, it returns the player's own `ordinal`, placements, `ratingUpdateCount` and `ratingChange`. It needs the player's own match credentials and never covers the opponent.
- The launcher's "rank display toggle" (`enableRankDisplay`) only shows or hides the user's own rank and rating on the Home page. It does nothing for opponents.

## The launcher's rank rules (what the crate copies)

- `calculate_rank.ts`: 0 sets gives None; 1 to 4 sets gives Pending; a rating of at least 2191.75 plus a daily global or regional placement gives Grandmaster; otherwise fixed rating floors from Bronze 1 to Master 3. A placement of `0` counts as no placement (JS truthiness).
- The launcher never calculates `banned`, but it does have a name and an icon for it.
- Colors come from `get_rank_details.ts`. None, Banned and Pending have no color.
- Icons: None, Banned and Pending use three different "Unranked" icons in the launcher. The repo's `assets/rank-icons/` has only `UNRANKED.svg` (the same file as the launcher's `rank_Unranked1.svg`) and `undefined.svg` (a byte-for-byte copy of it). To show Banned and Pending distinctly, copy `rank_Unranked2.svg` and `rank_Unranked3.svg` from the launcher source.

## For the integrator

- Use `eppi_rank::fetch(&codes)` from a worker thread. It batches 25 codes per request (`BATCH_SIZE`) and sends the batches one after another, with a 15 s timeout each.
- Cache results by normalized code. A rank changes at most once per set, so fetch each opponent once per session, or again after about 10 minutes. Don't re-fetch on every UI refresh.
- For a big backfill (hundreds of codes from old replays), call `fetch` with 25 codes at a time and wait about a second between calls, rather than passing everything at once.
- On `Error::Http { status: 429, retry_after }`, wait `retry_after`, or 60 s if the header is missing, then back off exponentially. On `Error::Network`, retry later. `Error::GraphQl` with status 400 means Slippi changed the schema, so retrying won't help. Log it and stop.
