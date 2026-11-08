//! Winners (slippi-js `utils/getWinners.ts`).

use peppi::game::immutable::Game;

use crate::frames::{port_columns, read_post};
use crate::types::Placement;

const END_UNRESOLVED: u8 = 0;
const END_TIME: u8 = 1;
const END_NO_CONTEST: u8 = 7;

/// Game End fields as slippi-js parses them from the raw event.
struct GameEnd {
    method: u8,
    lras_initiator: Option<i8>,
    /// Placement per port 0..=3 (`None` before Slippi 3.13).
    placements: [Option<i8>; 4],
}

impl GameEnd {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let byte = |i: usize| bytes.get(i).map(|&b| b as i8);
        Some(Self {
            method: *bytes.first()?,
            lras_initiator: byte(1),
            placements: [byte(2), byte(3), byte(4), byte(5)],
        })
    }
}

/// A player on the last frame (slippi-js "final post-frame update").
#[derive(Clone, Copy)]
struct FinalPost {
    player_index: u8,
    stocks: u8,
    percent: f64,
}

/// Settings player: index and team id (`team.color`, slippi-js `teamId`).
struct Player {
    index: u8,
    team: Option<u8>,
}

pub(crate) fn compute(game: &Game) -> Vec<Placement> {
    // slippi-js locates the final post-frame updates by stepping back from the end of the raw
    // stream past a Game End event. Without one that lands on the wrong bytes and it reports no
    // winner, so a replay without Game End has no winners here either.
    let Some(end) = game.end.as_ref().and_then(|e| GameEnd::parse(&e.bytes.0)) else {
        return Vec::new();
    };
    let players: Vec<Player> = game
        .start
        .players
        .iter()
        .map(|p| Player {
            index: u8::from(p.port),
            team: p.team.map(|t| t.color),
        })
        .collect();

    let finals: Vec<FinalPost> = match game.frames.id.len().checked_sub(1) {
        None => Vec::new(),
        Some(row) => port_columns(game)
            .iter()
            .zip(&players)
            .filter_map(|(col, p)| {
                let post = read_post(&game.frames.ports[(*col)?].leader, row)?;
                Some(FinalPost {
                    player_index: p.index,
                    stocks: post.stocks,
                    percent: f64::from(post.percent),
                })
            })
            .collect(),
    };
    winners(&end, &players, game.start.is_teams, &finals)
}

fn winners(
    end: &GameEnd,
    players: &[Player],
    is_teams: bool,
    finals: &[FinalPost],
) -> Vec<Placement> {
    // LRAS / unresolved: in 1v1, whoever didn't LRAS wins.
    if end.method == END_NO_CONTEST || end.method == END_UNRESOLVED {
        if let Some(lras) = end.lras_initiator
            && players.len() == 2
            && let Some(w) = players
                .iter()
                .find(|p| i16::from(p.index) != i16::from(lras))
        {
            return vec![Placement {
                player_index: w.index,
                position: Some(0),
            }];
        }
        return Vec::new();
    }

    // Everyone out of stocks: draw.
    if finals.iter().all(|f| f.stocks == 0) {
        return Vec::new();
    }

    // Timeouts: decide from the last frame.
    if end.method == END_TIME && finals.len() >= 2 {
        return if is_teams {
            teams_winners(players, finals)
        } else {
            ffa_winners(finals)
        };
    }

    // Placements.
    let placements: Vec<Placement> = end
        .placements
        .iter()
        .enumerate()
        .map(|(i, &position)| Placement {
            player_index: i as u8,
            position,
        })
        .collect();
    let Some(first) = placements.iter().find(|p| p.position == Some(0)) else {
        return Vec::new();
    };
    let team_of = |index: u8| players.iter().find(|p| p.index == index).map(|p| p.team);
    if is_teams && let Some(winning_team) = team_of(first.player_index) {
        return placements
            .iter()
            .filter(|p| team_of(p.player_index) == Some(winning_team))
            .copied()
            .collect();
    }
    vec![*first]
}

/// Stocks descending, then percent ascending (stable).
fn sort_by_performance(players: &mut [FinalPost]) {
    players.sort_by(|a, b| {
        b.stocks
            .cmp(&a.stocks)
            .then(a.percent.total_cmp(&b.percent))
    });
}

fn ffa_winners(finals: &[FinalPost]) -> Vec<Placement> {
    let mut sorted = finals.to_vec();
    sort_by_performance(&mut sorted);
    let top = sorted[0];
    let mut winners: Vec<FinalPost> = sorted
        .into_iter()
        .filter(|p| p.stocks == top.stocks && p.percent == top.percent)
        .collect();
    winners.sort_by_key(|p| p.player_index);
    winners
        .iter()
        .map(|p| Placement {
            player_index: p.player_index,
            position: Some(0),
        })
        .collect()
}

fn teams_winners(players: &[Player], finals: &[FinalPost]) -> Vec<Placement> {
    struct Team {
        id: i16,
        stocks: u32,
        percent: f64,
        players: Vec<FinalPost>,
    }
    let mut teams: Vec<Team> = Vec::new();
    for f in finals {
        let Some(player) = players.iter().find(|p| p.index == f.player_index) else {
            continue;
        };
        let id = player.team.map_or(-1, i16::from);
        let team = match teams.iter().position(|t| t.id == id) {
            Some(i) => &mut teams[i],
            None => {
                teams.push(Team {
                    id,
                    stocks: 0,
                    percent: 0.0,
                    players: Vec::new(),
                });
                teams.last_mut().expect("just pushed")
            }
        };
        team.stocks += u32::from(f.stocks);
        team.percent += f.percent;
        team.players.push(*f);
    }
    if teams.is_empty() {
        return Vec::new();
    }
    teams.sort_by(|a, b| {
        b.stocks
            .cmp(&a.stocks)
            .then(a.percent.total_cmp(&b.percent))
    });
    let mut winning = std::mem::take(&mut teams[0].players);
    let mut ranked = winning.clone();
    sort_by_performance(&mut ranked);
    winning.sort_by_key(|p| p.player_index);
    winning
        .iter()
        .map(|p| Placement {
            player_index: p.player_index,
            position: ranked
                .iter()
                .position(|r| r.player_index == p.player_index)
                .map(|i| i as i8),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(player_index: u8, stocks: u8, percent: f64) -> FinalPost {
        FinalPost {
            player_index,
            stocks,
            percent,
        }
    }

    fn singles() -> Vec<Player> {
        vec![
            Player {
                index: 0,
                team: None,
            },
            Player {
                index: 1,
                team: None,
            },
        ]
    }

    fn end(method: u8, lras: i8, placements: [i8; 4]) -> GameEnd {
        GameEnd {
            method,
            lras_initiator: Some(lras),
            placements: placements.map(Some),
        }
    }

    fn indices(w: &[Placement]) -> Vec<u8> {
        w.iter().map(|p| p.player_index).collect()
    }

    #[test]
    fn lras_quitter_loses() {
        let w = winners(
            &end(7, 0, [-1; 4]),
            &singles(),
            false,
            &[fp(0, 4, 0.0), fp(1, 1, 90.0)],
        );
        assert_eq!(indices(&w), vec![1]);
        // No initiator recorded (-1): slippi-js picks the first player whose index isn't -1.
        let w = winners(&end(7, -1, [-1; 4]), &singles(), false, &[]);
        assert_eq!(indices(&w), vec![0]);
    }

    #[test]
    fn placements_and_draws() {
        let finals = [fp(0, 0, 50.0), fp(1, 2, 10.0)];
        let w = winners(&end(2, -1, [1, 0, -1, -1]), &singles(), false, &finals);
        assert_eq!(indices(&w), vec![1]);
        let w = winners(
            &end(2, -1, [0, 1, -1, -1]),
            &singles(),
            false,
            &[fp(0, 0, 1.0), fp(1, 0, 2.0)],
        );
        assert!(w.is_empty());
    }

    #[test]
    fn timeout_uses_stocks_then_percent() {
        let w = winners(
            &end(1, -1, [-1; 4]),
            &singles(),
            false,
            &[fp(0, 2, 80.0), fp(1, 2, 30.0)],
        );
        assert_eq!(indices(&w), vec![1]);
        let w = winners(
            &end(1, -1, [-1; 4]),
            &singles(),
            false,
            &[fp(0, 2, 30.0), fp(1, 2, 30.0)],
        );
        assert_eq!(indices(&w), vec![0, 1]);
    }

    #[test]
    fn teams_timeout() {
        let players = vec![
            Player {
                index: 0,
                team: Some(0),
            },
            Player {
                index: 1,
                team: Some(1),
            },
            Player {
                index: 2,
                team: Some(0),
            },
            Player {
                index: 3,
                team: Some(1),
            },
        ];
        let finals = [fp(0, 1, 10.0), fp(1, 2, 50.0), fp(2, 1, 5.0), fp(3, 1, 0.0)];
        let w = winners(&end(1, -1, [-1; 4]), &players, true, &finals);
        assert_eq!(indices(&w), vec![1, 3]);
        // Positions rank players within the winning team: more stocks first.
        assert_eq!(w[0].position, Some(0));
        assert_eq!(w[1].position, Some(1));
    }
}
