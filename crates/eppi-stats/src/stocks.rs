//! Stocks (slippi-js `stocks.ts`).

use crate::frames::{Frames, Post};
use crate::melee::state::is_dead;
use crate::types::{Conversion, Stock};

/// slippi-js `didLoseStock`.
pub(crate) fn did_lose_stock(frame: &Post, prev: &Post) -> bool {
    i32::from(prev.stocks) - i32::from(frame.stocks) > 0
}

/// Computes stocks for singles permutations (`(player, opponent)` indices into `frames.players`).
pub(crate) fn compute(frames: &Frames, perms: &[(usize, usize)]) -> Vec<Stock> {
    let mut stocks: Vec<Stock> = Vec::new();
    // Index into `stocks` of each permutation's current stock.
    let mut current: Vec<Option<usize>> = vec![None; perms.len()];
    for k in 0..frames.len {
        let frame_number = Frames::frame_number(k);
        for (pi, &(p, _)) in perms.iter().enumerate() {
            let player = &frames.players[p];
            let post = &player.post[k];
            let prev = k.checked_sub(1).map(|j| &player.post[j]);
            match current[pi] {
                None => {
                    if is_dead(post.state) {
                        continue;
                    }
                    current[pi] = Some(stocks.len());
                    stocks.push(Stock {
                        player_index: player.player_index,
                        start_frame: frame_number,
                        end_frame: None,
                        start_percent: 0.0,
                        current_percent: 0.0,
                        end_percent: None,
                        count: post.stocks,
                        death_animation: None,
                        killed_by: None,
                        kill_move_id: None,
                    });
                }
                Some(si) => {
                    let stock = &mut stocks[si];
                    if let Some(prev) = prev.filter(|prev| did_lose_stock(post, prev)) {
                        stock.end_frame = Some(frame_number);
                        stock.end_percent = Some(f64::from(prev.percent));
                        stock.death_animation = Some(post.state);
                        current[pi] = None;
                    } else {
                        stock.current_percent = f64::from(post.percent);
                    }
                }
            }
        }
    }
    stocks
}

/// Fills [`Stock::killed_by`] / [`Stock::kill_move_id`] from the conversion that ended with
/// each death, the way the Slippi Launcher's kills table finds the kill move.
pub(crate) fn attribute_kills(stocks: &mut [Stock], conversions: &[Conversion]) {
    for stock in stocks.iter_mut() {
        let Some(end) = stock.end_frame else { continue };
        let killer = conversions.iter().find(|c| {
            c.did_kill && c.player_index == stock.player_index && c.end_frame == Some(end)
        });
        if let Some(c) = killer {
            stock.killed_by = c.last_hit_by;
            stock.kill_move_id = c.moves.last().map(|m| m.move_id);
        }
    }
}
