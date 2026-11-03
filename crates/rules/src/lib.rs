//! Game rules, kept headless so they can be tested without a window.
//!
//! Original formulas come from the imported tables; [`Tuning`] is the remake's
//! rebalance layer on top of them, loaded from `patches/*.toml`.

use serde::Deserialize;

const MODERN: &str = include_str!("../../../patches/modern.toml");

#[derive(Debug, Deserialize)]
pub struct Tuning {
    pub xp: Xp,
    pub spells: Spells,
    pub loot: Loot,
    pub boxes: Boxes,
}

#[derive(Debug, Deserialize)]
pub struct Xp {
    pub base: f64,
    pub exponent: f64,
    pub depth_bonus_per_floor: f64,
    pub catchup_per_level_behind: f64,
    pub catchup_cap: f64,
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

    pub fn xp_to_next(&self, level: u32) -> u64 {
        (self.xp.base * f64::from(level.max(1)).powf(self.xp.exponent)).round() as u64
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
        assert!(t.xp_to_next(10) > t.xp_to_next(9));
        assert!(t.magic_lock_chance(1) < 0.05 && t.magic_lock_chance(5) < 0.05);
        assert!(t.magic_lock_chance(15) > t.magic_lock_chance(6));
        assert!(t.xp_award(100, 10, 0) > t.xp_award(100, 1, 0));
    }

    #[test]
    fn patch_overrides_one_key() {
        let t = Tuning::load(&["[spells]\nmax_sp_mult = 4.0"]).unwrap();
        assert_eq!(t.spells.max_sp_mult, 4.0);
        assert_eq!(t.spells.regen_per_step, 0.005);
    }
}
