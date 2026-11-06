//! The city store's and Holding & Confinement's stock, shared by every character
//! (saves/store.json), and the original's prices (MORDOR.EXE): the more there is in
//! stock, the cheaper it is; items sold restock the store; resting in the city brings
//! new stock from the floors the party has reached.

use std::{io, path::Path};

use ddj_extract::vfs as fs;

use serde::{Deserialize, Serialize};

use crate::{
    game::{CHA, Character, Held},
    magic,
    mdr::{GameData, Words},
};

/// Store-only (level 0) items never run out.
pub const UNLIMITED: u8 = 99;
const FILE: &str = "store.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Store {
    /// Count per item (index into `GameData::items`).
    pub items: Vec<u8>,
    /// Count per monster (index into `GameData::monsters`).
    pub companions: Vec<u8>,
    /// The deepest floor reached: what can appear on the shelves.
    pub deepest: usize,
    /// When it was last saved (Unix seconds): cloud saves keep the newer store.
    #[serde(default)]
    pub saved: u64,
}

impl Store {
    /// The opening stock (MORDOR.EXE): the six special items once each; every
    /// uncursed level-0 item without limit and level-1 item five times, up to 200,000
    /// gold; and two of the first monsters of six kinds in Confinement.
    pub fn fresh(d: &GameData) -> Self {
        let items = (0..d.items.len())
            .map(|i| {
                let it = &d.items[i];
                match () {
                    _ if (356..=361).contains(&it.id) => 1,
                    _ if it.level > 1 || it.price == 0 || it.price > 200_000 || it.w(40) != 0 => 0,
                    _ if it.level == 0 => UNLIMITED,
                    _ => 5,
                }
            })
            .collect();
        let mut companions = vec![0; d.monsters.len()];
        for (kind, n) in [("Humanoid", 2), ("Animal", 2), ("Insect", 2), ("Thief", 2), ("Clean-Up", 1), ("Undead", 1)] {
            let Some(k) = d.monster_types.iter().position(|t| t.starts_with(kind)) else {
                continue;
            };
            for m in (1..d.monsters.len()).filter(|&m| d.monsters[m].kind as usize == k).take(n) {
                companions[m] = 2;
            }
        }
        Self { items, companions, deepest: 1, saved: 0 }
    }

    pub fn load(dir: &Path, d: &GameData) -> Self {
        let s: Option<Self> = fs::read(dir.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok());
        s.filter(|s| s.items.len() == d.items.len() && s.companions.len() == d.monsters.len()).unwrap_or_else(|| Self::fresh(d))
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(FILE), serde_json::to_vec(self)?)
    }

    pub fn count(&self, item: usize) -> u8 {
        self.items.get(item).copied().unwrap_or(0)
    }

    /// What the store has, by kind then price.
    pub fn listed(&self, d: &GameData) -> Vec<usize> {
        let mut v: Vec<usize> = (0..d.items.len()).filter(|&i| self.count(i) > 0).collect();
        v.sort_by_key(|&i| (d.items[i].kind, d.items[i].price));
        v
    }

    /// One bought: limited stock goes down.
    pub(crate) fn take(&mut self, item: usize) {
        if let Some(n) = self.items.get_mut(item).filter(|n| (1..UNLIMITED).contains(*n)) {
            *n -= 1;
        }
    }

    /// One sold: back on the shelf unless cursed, store-only or partly used.
    pub(crate) fn put(&mut self, d: &GameData, h: &Held) {
        let it = &d.items[h.item];
        let used = magic::consumable(d, h.item) && h.charges < it.w(13);
        if let Some(n) = self.items.get_mut(h.item).filter(|n| **n < 9)
            && it.level > 0
            && it.w(40) == 0
            && !used
        {
            *n += 1;
        }
    }

    /// Resting `days` in the city restocks (MORDOR.EXE): while d100 <= days (50 at most)
    /// one more event, 60% bringing an item (and a monster to Confinement), 40% selling
    /// one off. Deeper floors reached bring deeper finds, the shallow ones likelier.
    pub fn restock(&mut self, d: &GameData, rng: &mut Character, days: u32) {
        let p = f64::from(days.clamp(1, 50));
        let deep = self.deepest.clamp(1, 14) as i32;
        while rng.rnd() * 100.0 <= p {
            let add = rng.rnd() * 100.0 < 60.0;
            let r = rng.d100();
            let level = (1..=deep).filter(|&a| f64::from(r) < 100.0 * (1.0 - f64::from(a) / f64::from(deep + 1))).count().clamp(1, deep as usize) as i16;
            let kind = (rng.rnd() * 35.0).round_ties_even() as i16;
            let items: Vec<(usize, i16)> =
                (0..d.items.len()).filter(|&i| d.items[i].kind == kind && (1..=level).contains(&d.items[i].level) && d.items[i].w(40) == 0).map(|i| (i, d.items[i].w(6))).collect();
            if let Some(i) = weighted(rng, &items) {
                let n = &mut self.items[i];
                if add && *n < 9 {
                    *n += 1;
                } else if !add && (1..UNLIMITED).contains(n) {
                    *n -= 1;
                }
            }
            let kind = (rng.rnd() * 18.0).round_ties_even() as i16;
            let monsters: Vec<(usize, i16)> =
                (0..d.monsters.len()).filter(|&m| d.monsters[m].kind == kind && (1..=level).contains(&d.monsters[m].level)).map(|m| (m, d.monsters[m].weight)).collect();
            if let Some(m) = weighted(rng, &monsters) {
                let n = &mut self.companions[m];
                if add && *n < 9 {
                    *n += 1;
                } else if !add && *n > 0 {
                    *n -= 1;
                }
            }
        }
    }
}

/// A pick weighted by the table's rarity weights.
fn weighted(rng: &mut Character, v: &[(usize, i16)]) -> Option<usize> {
    let total: u32 = v.iter().map(|&(_, w)| w.max(0) as u32).sum();
    let mut r = rng.roll(total.max(1));
    v.iter()
        .find(|&&(_, w)| {
            let w = w.max(0) as u32;
            let hit = r < w;
            r = r.saturating_sub(w);
            hit
        })
        .map(|&(i, _)| i)
}

fn stock(s: u8) -> f64 {
    f64::from(s.clamp(1, 9))
}

/// The store's price with `s` in stock: a store-only item's own price, otherwise
/// 1.5 P / (ln s + 1.15): 1.3 P for the last one, 0.45 P with nine.
pub fn buy_price(d: &GameData, item: usize, s: u8) -> u64 {
    let p = f64::from(d.items[item].price);
    if d.items[item].level == 0 { p as u64 } else { (1.5 * p / (stock(s).ln() + 1.15)).round_ties_even() as u64 }
}

/// What the store pays: 0.9 P / (s + 1.15), a third of that for an unidentified item,
/// less 5% per unit of ln(charges used + 1) (x5 for potions, scrolls and the like).
/// Known cursed items fetch nothing.
pub fn sell_price(d: &GameData, h: &Held, s: u8) -> u64 {
    let it = &d.items[h.item];
    if h.known && it.w(40) != 0 {
        return 0;
    }
    let base = (0.9 * f64::from(it.price) / (f64::from(s.min(9)) + 1.15) / if h.known { 1.0 } else { 3.0 }).round_ties_even();
    let full = it.w(13);
    let charged = full > 0 && matches!(it.kind, 23..=28 | 30 | 31);
    let used = f64::from((full - if charged { h.charges.min(full) } else { h.charges }).abs());
    let m = if charged { 5.0 } else { 1.0 } * (used + 1.0).ln();
    (base - m * 0.05 * base).max(0.0).round_ties_even() as u64
}

/// Identifying an unknown item: the original's two steps, 0.23 P / ((s/2 + 1) 0.9) / step.
pub fn identify_price(d: &GameData, item: usize, s: u8) -> u64 {
    let p = 0.23 * f64::from(d.items[item].price) / ((f64::from(s.min(9)) / 2.0 + 1.0) * 0.9);
    (p / 2.0 + p + 2.0).round_ties_even() as u64
}

/// Uncursing: 1.6 x the price.
pub fn uncurse_price(d: &GameData, item: usize) -> u64 {
    (1.6 * f64::from(d.items[item].price)).round_ties_even() as u64
}

/// CHA's passive (docs/balance.md): 1% better a point above 10, buying and selling.
fn cha(c: &Character) -> f64 {
    f64::from(c.stats[CHA] - 10).max(0.0) / 100.0
}

pub(crate) fn cha_buy(c: &Character, price: u64) -> u64 {
    (price as f64 * (1.0 - cha(c))).round() as u64
}

pub(crate) fn cha_sell(c: &Character, price: u64) -> u64 {
    (price as f64 * (1.0 + cha(c))).round() as u64
}
