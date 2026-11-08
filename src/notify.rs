//! A desktop notification when a game starts while Eppi is in the
//! background: who you're playing, their rank and your record with them.

use crate::app::{Data, display_rank};
use crate::model::{Game, Outcome, Player};

pub fn game_started(game: &Game, opponent: &Player, data: &Data) {
    let rank = data
        .rank(&opponent.code)
        .and_then(|r| r.rank.as_deref())
        .map(display_rank);
    let (mut wins, mut losses) = (0, 0);
    for g in &data.games {
        if g.opponent(&data.me)
            .is_some_and(|p| p.code == opponent.code)
        {
            match g.outcome(&data.me) {
                Outcome::Win => wins += 1,
                Outcome::Loss => losses += 1,
                Outcome::Unknown => {}
            }
        }
    }
    let mut body = format!(
        "{} · {}",
        crate::melee::character_name(opponent.character),
        crate::melee::stage_name(game.stage)
    );
    if let Some(rank) = rank {
        body.push_str(&format!(" · {rank}"));
    }
    if wins + losses > 0 {
        body.push_str(&format!("\nYou're {wins}–{losses} in games against them"));
    } else {
        body.push_str("\nFirst time playing them");
    }
    if let Some(note) = data.notes.get(&opponent.code) {
        body.push_str(&format!("\n{note}"));
    }
    let summary = format!("{} vs {}", game.mode.label(), opponent.display_name());
    std::thread::spawn(move || {
        // From Eppi.app (scripts/install.sh), notifications are Eppi's own;
        // a bare binary keeps macOS's default sender.
        #[cfg(target_os = "macos")]
        {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                let _ = notify_rust::set_application("io.github.rj-jones.eppi");
            });
        }
        if let Err(e) = notify_rust::Notification::new()
            .summary(&summary)
            .body(&body)
            .appname("Eppi")
            .show()
        {
            log::warn!("notification: {e}");
        }
    });
}
