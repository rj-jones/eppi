//! Slippi ranked tiers, matching the official launcher.
//!
//! Thresholds come from the launcher's `calculate_rank.ts`, names and colors
//! from `get_rank_details.ts`, and icons from `get_rank_icon.ts`.

use std::fmt;

/// Sets a player must finish before the launcher shows a real rank.
pub const SETS_REQUIRED_FOR_RANK: u32 = 5;

/// Lowest rating that counts as Master 1, and the floor for Grandmaster.
pub const GRANDMASTER_MIN_RATING: f64 = 2191.75;

/// Color for None, Banned and Pending. The launcher gives those no color of
/// its own (it shows the rating only for real ranks), so this neutral gray is
/// Eppi's choice.
pub const UNRANKED_COLOR: u32 = 0x8A8A8A;

/// A ranked tier as the Slippi launcher shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rank {
    /// No ranked sets this season.
    None,
    /// Account banned. The launcher has this tier but never computes it from
    /// a rating; see [`crate::Lookup`] for when Eppi reports it.
    Banned,
    /// Fewer than [`SETS_REQUIRED_FOR_RANK`] sets this season.
    Pending,
    Bronze1,
    Bronze2,
    Bronze3,
    Silver1,
    Silver2,
    Silver3,
    Gold1,
    Gold2,
    Gold3,
    Platinum1,
    Platinum2,
    Platinum3,
    Diamond1,
    Diamond2,
    Diamond3,
    Master1,
    Master2,
    Master3,
    Grandmaster,
}

/// Rating floors from highest to lowest; Grandmaster is handled separately
/// because it also needs a daily placement. Anything below the last floor is
/// Bronze 1.
const FLOORS: [(f64, Rank); 17] = [
    (2350.0, Rank::Master3),
    (2275.0, Rank::Master2),
    (GRANDMASTER_MIN_RATING, Rank::Master1),
    (2136.28, Rank::Diamond3),
    (2073.67, Rank::Diamond2),
    (2003.92, Rank::Diamond1),
    (1927.03, Rank::Platinum3),
    (1843.0, Rank::Platinum2),
    (1751.83, Rank::Platinum1),
    (1653.52, Rank::Gold3),
    (1548.07, Rank::Gold2),
    (1435.48, Rank::Gold1),
    (1315.75, Rank::Silver3),
    (1188.88, Rank::Silver2),
    (1054.87, Rank::Silver1),
    (913.72, Rank::Bronze3),
    (765.43, Rank::Bronze2),
];

impl Rank {
    /// Every tier, lowest first.
    pub const ALL: [Rank; 22] = [
        Rank::None,
        Rank::Banned,
        Rank::Pending,
        Rank::Bronze1,
        Rank::Bronze2,
        Rank::Bronze3,
        Rank::Silver1,
        Rank::Silver2,
        Rank::Silver3,
        Rank::Gold1,
        Rank::Gold2,
        Rank::Gold3,
        Rank::Platinum1,
        Rank::Platinum2,
        Rank::Platinum3,
        Rank::Diamond1,
        Rank::Diamond2,
        Rank::Diamond3,
        Rank::Master1,
        Rank::Master2,
        Rank::Master3,
        Rank::Grandmaster,
    ];

    /// The tier for a ranked profile, exactly as the launcher's
    /// `calculateRank` computes it. `has_placement` means the player has a
    /// daily global or regional placement.
    pub fn from_profile(rating: f64, sets_played: u32, has_placement: bool) -> Rank {
        if sets_played == 0 {
            return Rank::None;
        }
        if sets_played < SETS_REQUIRED_FOR_RANK {
            return Rank::Pending;
        }
        if rating >= GRANDMASTER_MIN_RATING && has_placement {
            return Rank::Grandmaster;
        }
        FLOORS
            .iter()
            .find(|(floor, _)| rating >= *floor)
            .map_or(Rank::Bronze1, |&(_, rank)| rank)
    }

    /// Display name, as the launcher spells it ("Platinum 2", "Grandmaster").
    pub fn name(self) -> &'static str {
        match self {
            Rank::None => "None",
            Rank::Banned => "Banned",
            Rank::Pending => "Pending",
            Rank::Bronze1 => "Bronze 1",
            Rank::Bronze2 => "Bronze 2",
            Rank::Bronze3 => "Bronze 3",
            Rank::Silver1 => "Silver 1",
            Rank::Silver2 => "Silver 2",
            Rank::Silver3 => "Silver 3",
            Rank::Gold1 => "Gold 1",
            Rank::Gold2 => "Gold 2",
            Rank::Gold3 => "Gold 3",
            Rank::Platinum1 => "Platinum 1",
            Rank::Platinum2 => "Platinum 2",
            Rank::Platinum3 => "Platinum 3",
            Rank::Diamond1 => "Diamond 1",
            Rank::Diamond2 => "Diamond 2",
            Rank::Diamond3 => "Diamond 3",
            Rank::Master1 => "Master 1",
            Rank::Master2 => "Master 2",
            Rank::Master3 => "Master 3",
            Rank::Grandmaster => "Grandmaster",
        }
    }

    /// Tier color as `0xRRGGBB`, from the launcher. None, Banned and Pending
    /// have no launcher color and get [`UNRANKED_COLOR`].
    pub fn color(self) -> u32 {
        match self {
            Rank::None | Rank::Banned | Rank::Pending => UNRANKED_COLOR,
            Rank::Bronze1 | Rank::Bronze2 | Rank::Bronze3 => 0xE06A36,
            Rank::Silver1 | Rank::Silver2 | Rank::Silver3 => 0xB5A5B7,
            Rank::Gold1 | Rank::Gold2 | Rank::Gold3 => 0xF6A51E,
            Rank::Platinum1 | Rank::Platinum2 | Rank::Platinum3 => 0x91E8E0,
            Rank::Diamond1 | Rank::Diamond2 | Rank::Diamond3 => 0x5DDFF4,
            Rank::Master1 | Rank::Master2 | Rank::Master3 => 0x6847BA,
            Rank::Grandmaster => 0xE51D13,
        }
    }

    /// File name of the tier's icon in the repo's `assets/rank-icons/`.
    ///
    /// The launcher uses three different unranked icons (None, Banned,
    /// Pending); the repo only has the first, `UNRANKED.svg`, so all three
    /// map to it.
    pub fn icon_file(self) -> &'static str {
        match self {
            Rank::None | Rank::Banned | Rank::Pending => "UNRANKED.svg",
            Rank::Bronze1 => "BRONZE 1.svg",
            Rank::Bronze2 => "BRONZE 2.svg",
            Rank::Bronze3 => "BRONZE 3.svg",
            Rank::Silver1 => "SILVER 1.svg",
            Rank::Silver2 => "SILVER 2.svg",
            Rank::Silver3 => "SILVER 3.svg",
            Rank::Gold1 => "GOLD 1.svg",
            Rank::Gold2 => "GOLD 2.svg",
            Rank::Gold3 => "GOLD 3.svg",
            Rank::Platinum1 => "PLATINUM 1.svg",
            Rank::Platinum2 => "PLATINUM 2.svg",
            Rank::Platinum3 => "PLATINUM 3.svg",
            Rank::Diamond1 => "DIAMOND 1.svg",
            Rank::Diamond2 => "DIAMOND 2.svg",
            Rank::Diamond3 => "DIAMOND 3.svg",
            Rank::Master1 => "MASTER 1.svg",
            Rank::Master2 => "MASTER 2.svg",
            Rank::Master3 => "MASTER 3.svg",
            Rank::Grandmaster => "GRANDMASTER.svg",
        }
    }

    /// True for Bronze 1 and up; false for None, Banned and Pending. The
    /// launcher hides the rating number when this is false.
    pub fn is_ranked(self) -> bool {
        !matches!(self, Rank::None | Rank::Banned | Rank::Pending)
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RANKED: u32 = SETS_REQUIRED_FOR_RANK;

    #[test]
    fn set_count_gates_come_first() {
        // Zero sets is None even with a huge rating and a placement.
        assert_eq!(Rank::from_profile(3000.0, 0, true), Rank::None);
        assert_eq!(Rank::from_profile(1100.0, 0, false), Rank::None);
        // 1 to 4 sets is Pending.
        assert_eq!(Rank::from_profile(1100.0, 1, false), Rank::Pending);
        assert_eq!(Rank::from_profile(3000.0, 4, true), Rank::Pending);
        // The fifth set unlocks a real rank.
        assert_eq!(Rank::from_profile(1100.0, 5, false), Rank::Silver1);
    }

    #[test]
    fn every_floor_is_inclusive() {
        for &(floor, rank) in &FLOORS {
            assert_eq!(Rank::from_profile(floor, RANKED, false), rank, "at {floor}");
            let below = Rank::from_profile(floor - 0.01, RANKED, false);
            assert!(
                below < rank,
                "{below:?} just under {floor} should be below {rank:?}"
            );
        }
    }

    #[test]
    fn each_boundary_lands_in_the_next_tier_down() {
        let cases = [
            (2349.99, Rank::Master2),
            (2274.99, Rank::Master1),
            (2191.74, Rank::Diamond3),
            (2136.27, Rank::Diamond2),
            (2073.66, Rank::Diamond1),
            (2003.91, Rank::Platinum3),
            (1927.02, Rank::Platinum2),
            (1842.99, Rank::Platinum1),
            (1751.82, Rank::Gold3),
            (1653.51, Rank::Gold2),
            (1548.06, Rank::Gold1),
            (1435.47, Rank::Silver3),
            (1315.74, Rank::Silver2),
            (1188.87, Rank::Silver1),
            (1054.86, Rank::Bronze3),
            (913.71, Rank::Bronze2),
            (765.42, Rank::Bronze1),
            (0.0, Rank::Bronze1),
            (-50.0, Rank::Bronze1),
        ];
        for (rating, rank) in cases {
            assert_eq!(
                Rank::from_profile(rating, RANKED, false),
                rank,
                "at {rating}"
            );
        }
    }

    #[test]
    fn grandmaster_needs_master_rating_and_a_placement() {
        assert_eq!(Rank::from_profile(2191.75, RANKED, true), Rank::Grandmaster);
        assert_eq!(Rank::from_profile(2600.0, RANKED, true), Rank::Grandmaster);
        // Just under the Master floor: a placement doesn't help.
        assert_eq!(Rank::from_profile(2191.74, RANKED, true), Rank::Diamond3);
        // No placement: plain Master tiers.
        assert_eq!(Rank::from_profile(2191.75, RANKED, false), Rank::Master1);
        assert_eq!(Rank::from_profile(2600.0, RANKED, false), Rank::Master3);
    }

    #[test]
    fn nan_rating_falls_to_bronze_1() {
        assert_eq!(Rank::from_profile(f64::NAN, RANKED, true), Rank::Bronze1);
    }

    #[test]
    fn icons_exist_in_the_assets_folder() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/rank-icons");
        for rank in Rank::ALL {
            let path = dir.join(rank.icon_file());
            assert!(
                path.is_file(),
                "missing icon for {rank:?}: {}",
                path.display()
            );
        }
    }

    #[test]
    fn names_colors_and_ranked_flag() {
        assert_eq!(Rank::Platinum2.name(), "Platinum 2");
        assert_eq!(Rank::Grandmaster.to_string(), "Grandmaster");
        assert_eq!(Rank::Gold3.color(), 0xF6A51E);
        assert_eq!(Rank::Pending.color(), UNRANKED_COLOR);
        assert!(!Rank::Pending.is_ranked());
        assert!(Rank::Bronze1.is_ranked());
        // ALL is sorted and complete.
        assert!(Rank::ALL.windows(2).all(|w| w[0] < w[1]));
    }
}
