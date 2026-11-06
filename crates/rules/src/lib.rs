//! Game rules, kept headless so they can be tested without a window.
//!
//! Original formulas come from the imported tables; [`Tuning`] is the remake's
//! rebalance layer on top of them, loaded from `patches/*.toml`.

pub mod ai;
pub mod combat;
pub mod game;
pub mod hall;
pub mod magic;
pub mod mdata4;
pub mod mdr;
pub mod monsters;
pub mod store;
pub mod treasure;

use serde::Deserialize;

const MODERN: &str = include_str!("../../../patches/modern.toml");

#[derive(Debug, Deserialize)]
pub struct Tuning {
    pub xp: Xp,
    pub spells: Spells,
    pub loot: Loot,
    pub combat: Combat,
    pub boxes: Boxes,
}

#[derive(Debug, Deserialize)]
pub struct Xp {
    pub divisor_early: f64,
    pub divisor_late: f64,
    pub ease_from: u32,
    pub ease_to: u32,
    pub depth_bonus_per_floor: f64,
    pub catchup_per_level_behind: f64,
    pub catchup_cap: f64,
}

#[derive(Debug, Deserialize)]
pub struct Combat {
    pub round_seconds: f64,
}

#[derive(Debug, Deserialize)]
pub struct Spells {
    pub max_sp_mult: f64,
    pub regen_per_step: f64,
}

#[derive(Debug, Deserialize)]
pub struct Loot {
    pub sp_potion_vs_heal: f64,
}

#[derive(Debug, Deserialize)]
pub struct Boxes {
    pub magic_lock_chance: Vec<f64>,
}

impl Tuning {
    /// The built-in modern rebalance with `patches` (TOML text) applied in order.
    pub fn load(patches: &[&str]) -> Result<Self, toml::de::Error> {
        let mut base: toml::Table = MODERN.parse()?;
        for p in patches {
            merge(&mut base, p.parse()?);
        }
        base.try_into()
    }

    /// Total XP a character at guild `level` needs before making the next level: the
    /// original's per-level costs, each divided by that level's divisor.
    pub fn xp_needed(&self, level: u32, race_xp: f32, guild_xp: f32) -> u64 {
        let x = &self.xp;
        let total: f64 = (1..=level.max(1)).map(|l| {
            let step = original_xp(l, race_xp, guild_xp) - if l > 1 { original_xp(l - 1, race_xp, guild_xp) } else { 0.0 };
            let t = (f64::from(l.saturating_sub(x.ease_from)) / f64::from((x.ease_to - x.ease_from).max(1))).min(1.0);
            step / (x.divisor_early + (x.divisor_late - x.divisor_early) * t)
        }).sum();
        total.round() as u64
    }

    /// Gold the guild charges to make the next level (the original's, unchanged):
    /// MORDOR.EXE sums round(30.1 (2l - 1)) over the levels, then adds the guild's
    /// `gold_factor` tenths (Nomad 3: 39.13 L²).
    pub fn level_gold(level: u32, gold_factor: i16) -> u64 {
        let l = f64::from(level);
        let sum = (1..=level).fold(0.0, |acc: f64, i| (acc + l / 2.0 * f64::from(2 * i - 1) * 60.2 / l).round_ties_even());
        (sum + sum * f64::from(gold_factor) / 10.0).round_ties_even() as u64
    }

    /// XP for one character from a kill worth `monster_xp` on `floor`.
    pub fn xp_award(&self, monster_xp: u64, floor: u32, levels_behind: u32) -> u64 {
        let depth = 1.0 + self.xp.depth_bonus_per_floor * f64::from(floor.saturating_sub(1));
        let catchup = 1.0 + (self.xp.catchup_per_level_behind * f64::from(levels_behind)).min(self.xp.catchup_cap);
        (monster_xp as f64 * depth * catchup).round() as u64
    }

    pub fn magic_lock_chance(&self, floor: u32) -> f64 {
        let t = &self.boxes.magic_lock_chance;
        t.get(floor.saturating_sub(1) as usize).or(t.last()).copied().unwrap_or(0.0)
    }
}

/// The original v1.1 total XP to make level `level + 1`, as MORDOR.EXE computes it:
/// 8.33 (G L² + 8.256 L - 4.152) times a race term, with the VB Currency rounding
/// (4 decimals) of each step. It matches every reading taken in the game.
pub fn original_xp(level: u32, race_xp: f32, guild_xp: f32) -> f64 {
    let cur = |x: f64| (x * 10_000.0).round_ties_even() / 10_000.0;
    let (l, g, r) = (f64::from(level), f64::from(guild_xp), f64::from(race_xp + 1.0));
    let (a2, a4) = (cur(2.0 * l * g), cur(2.0 * l - 1.0));
    let base = if level == 0 { 0.0 } else { cur((a2 / 12.0 * 500.0 * l + 344.0 * a4 - 2.0) / 10.0) };
    cur(base * (r.ln() / (r + 1.0).ln() + 1.0 - 0.7 + r / 25.0)).round_ties_even()
}

/// Key-by-key overlay: tables merge recursively, anything else replaces.
fn merge(base: &mut toml::Table, patch: toml::Table) {
    for (k, v) in patch {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(p)) => merge(b, p),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_defaults_load() {
        let t = Tuning::load(&[]).unwrap();
        assert!(t.xp_needed(10, 3.0, 8.0) > t.xp_needed(9, 3.0, 8.0));
        assert!(t.magic_lock_chance(1) < 0.05 && t.magic_lock_chance(5) < 0.05);
        assert!(t.magic_lock_chance(15) > t.magic_lock_chance(6));
        assert!(t.xp_award(100, 10, 0) > t.xp_award(100, 1, 0));
    }

    /// Readings from v1.1 (Human = 3, Nomad = 8) and the decided divisor.
    #[test]
    fn xp_matches_original_and_divisor() {
        for (level, want) in [(1, 133.0), (10, 9672.0), (100, 889947.0)] {
            assert_eq!(original_xp(level, 3.0, 8.0), want, "level {level}");
        }
        assert_eq!((Tuning::level_gold(1, 3), Tuning::level_gold(10, 3)), (39, 3913));
        let t = Tuning::load(&[]).unwrap();
        let warrior = |l| t.xp_needed(l, 3.0, 16.0) as f64;
        assert!((warrior(60) / original_xp(60, 3.0, 16.0) - 0.4).abs() < 0.001);
        assert!((warrior(100) - 978_000.0).abs() < 2_000.0);
    }

    #[test]
    fn patch_overrides_one_key() {
        let t = Tuning::load(&["[spells]\nmax_sp_mult = 4.0"]).unwrap();
        assert_eq!(t.spells.max_sp_mult, 4.0);
        assert_eq!(t.spells.regen_per_step, 0.005);
    }
}
