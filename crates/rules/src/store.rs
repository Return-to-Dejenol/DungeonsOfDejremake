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
    /// Count per item (index into `GameData::items`) and alignment (none, good, neutral,
    /// evil), as MORDOR.EXE keeps it.
    pub items: Vec<[u8; 4]>,
    /// Count per monster (index into `GameData::monsters`).
    pub companions: Vec<u8>,
    /// The deepest floor reached: what can appear on the shelves.
    pub deepest: usize,
    /// When it was last saved (Unix seconds): cloud saves keep the newer store.
    #[serde(default)]
    pub saved: u64,
}

impl Store {
    /// The opening stock (MORDOR.EXE, sub05C2): the six special items once each; of the
    /// uncursed level 0-1 items up to 200,000 gold, an aligned one once in each alignment,
    /// an unaligned one without limit (level 0) or five times; and two of the first
    /// monsters of six kinds in Confinement.
    pub fn fresh(d: &GameData) -> Self {
        let items = (0..d.items.len())
            .map(|i| {
                let it = &d.items[i];
                match () {
                    _ if (356..=361).contains(&it.id) => [1, 0, 0, 0],
                    _ if it.level > 1 || it.price == 0 || it.price > 200_000 || it.w(40) != 0 => [0; 4],
                    _ if it.long(20) > 1 => [0, 1, 1, 1],
                    _ if it.level == 0 => [UNLIMITED, 0, 0, 0],
                    _ => [5, 0, 0, 0],
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

    pub fn count(&self, item: usize, al: u8) -> u8 {
        self.items.get(item).map_or(0, |n| n[usize::from(al.min(3))])
    }

    /// The stock a sale is priced on (fn273B): its alignment's, or 1 when only another
    /// alignment has some.
    pub fn sell_count(&self, h: &Held) -> u8 {
        let n = self.count(h.item, h.align);
        if n == 0 && (0..4).any(|a| self.count(h.item, a) > 0) { 1 } else { n }
    }

    /// What the store has in alignment `al`, by kind then price.
    pub fn listed(&self, d: &GameData, al: u8) -> Vec<usize> {
        let mut v: Vec<usize> = (0..d.items.len()).filter(|&i| self.count(i, al) > 0).collect();
        v.sort_by_key(|&i| (d.items[i].kind, d.items[i].price));
        v
    }

    /// One bought: limited stock goes down.
    pub(crate) fn take(&mut self, item: usize, al: u8) {
        if let Some(n) = self.items.get_mut(item).map(|n| &mut n[usize::from(al.min(3))]).filter(|n| (1..UNLIMITED).contains(*n)) {
            *n -= 1;
        }
    }

    /// One sold: back on its alignment's shelf unless cursed, store-only or partly used.
    pub(crate) fn put(&mut self, d: &GameData, h: &Held) {
        let it = &d.items[h.item];
        let used = magic::consumable(d, h.item) && h.charges < it.w(13);
        if let Some(n) = self.items.get_mut(h.item).map(|n| &mut n[usize::from(h.align.min(3))]).filter(|n| **n < 9)
            && it.level > 0
            && it.w(40) == 0
            && !used
        {
            *n += 1;
        }
    }

    /// Resting `days` in the city restocks (mdrReplenishStore, mdrReplenishCStore): the
    /// store and Confinement each run events while d100 <= days (50 at most), 60%
    /// bringing one (retried until a fit is found) and 40% selling one off (up to 100
    /// tries for one in stock). Deeper floors reached bring deeper finds, the shallow
    /// ones likelier; an item comes in an alignment its mask allows.
    pub fn restock(&mut self, d: &GameData, rng: &mut Character, days: u32) {
        let p = f64::from(days.clamp(1, 50));
        let deep = self.deepest.clamp(1, 14) as i32;
        let level = |rng: &mut Character| {
            let r = rng.d100();
            (1..=deep).filter(|&a| f64::from(r) < 100.0 * (1.0 - f64::from(a) / f64::from(deep + 1))).count().clamp(1, deep as usize) as i16
        };
        while rng.rnd() * 100.0 <= p {
            let (add, level) = (rng.rnd() * 100.0 < 60.0, level(rng));
            for _ in 0..if add { 1000 } else { 100 } {
                let kind = (rng.rnd() * d.item_types.len() as f64).round_ties_even() as i16;
                let items: Vec<(usize, i16)> =
                    (0..d.items.len()).filter(|&i| d.items[i].kind == kind && (1..=level).contains(&d.items[i].level) && d.items[i].w(40) == 0).map(|i| (i, d.items[i].w(6))).collect();
                let Some(i) = weighted(rng, &items) else { continue };
                let al = if add { usize::from(magic::roll_align(rng, d, i)) } else { (0..4).find(|&a| (1..UNLIMITED).contains(&self.items[i][a])).unwrap_or(4) };
                if al < 4 {
                    let n = &mut self.items[i][al];
                    if add { *n = (*n + 1).min(9) } else { *n -= 1 }
                    break;
                }
            }
        }
        while rng.rnd() * 100.0 <= p {
            let (add, level) = (rng.rnd() * 100.0 < 60.0, level(rng));
            for _ in 0..if add { 1000 } else { 100 } {
                let kind = (rng.rnd() * d.monster_types.len() as f64).round_ties_even() as i16;
                let monsters: Vec<(usize, i16)> =
                    (0..d.monsters.len()).filter(|&m| d.monsters[m].kind == kind && (1..=level).contains(&d.monsters[m].level)).map(|m| (m, d.monsters[m].weight)).collect();
                let Some(m) = weighted(rng, &monsters) else { continue };
                let n = &mut self.companions[m];
                if add || *n > 0 {
                    if add { *n = (*n + 1).min(9) } else { *n -= 1 }
                    break;
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

/// The store's price with `s` in stock: an unaligned store-only item's own price,
/// otherwise 1.5 P / (ln s + 1.15): 1.3 P for the last one, 0.45 P with nine.
pub fn buy_price(d: &GameData, item: usize, al: u8, s: u8) -> u64 {
    let p = f64::from(d.items[item].price);
    if d.items[item].level == 0 && al == 0 { p as u64 } else { (1.5 * p / (stock(s).ln() + 1.15)).round_ties_even() as u64 }
}

/// What the store pays (fn273B): 0.9 P / (s + 1.15) / (identification level + 1), less
/// 5% per unit of ln(charges used + 1) (x5 for potions, scrolls and the like). Known
/// cursed items fetch nothing.
pub fn sell_price(d: &GameData, h: &Held, s: u8) -> u64 {
    let it = &d.items[h.item];
    if h.known() && it.w(40) != 0 {
        return 0;
    }
    let base = (0.9 * f64::from(it.price) / (f64::from(s.min(9)) + 1.15) / (f64::from(h.id) + 1.0)).round_ties_even();
    let full = it.w(13);
    let charged = full > 0 && matches!(it.kind, 23..=28 | 30 | 31);
    let used = f64::from((full - if charged { h.charges.min(full) } else { h.charges }).abs());
    let m = if charged { 5.0 } else { 1.0 } * (used + 1.0).ln();
    (base - m * 0.05 * base).max(0.0).round_ties_even() as u64
}

/// One identification step (fn271B): 0.23 P / ((s/2 + 1) x 0.9) / level + 1.
pub fn identify_price(d: &GameData, h: &Held, s: u8) -> u64 {
    if h.known() {
        return 0;
    }
    (0.23 * f64::from(d.items[h.item].price) / ((f64::from(s.min(9)) / 2.0 + 1.0) * 0.9) / f64::from(h.id) + 1.0).round_ties_even() as u64
}

/// Uncursing: 1.6 x the price.
pub fn uncurse_price(d: &GameData, item: usize) -> u64 {
    (1.6 * f64::from(d.items[item].price)).round_ties_even() as u64
}

/// CHA's passive (docs/balance.md): 1% better a point above 10, buying and selling.
fn cha(c: &Character, d: &GameData) -> f64 {
    f64::from(c.stat(d, CHA) - 10).max(0.0) / 100.0
}

pub(crate) fn cha_buy(c: &Character, d: &GameData, price: u64) -> u64 {
    (price as f64 * (1.0 - cha(c, d))).round() as u64
}

pub(crate) fn cha_sell(c: &Character, d: &GameData, price: u64) -> u64 {
    (price as f64 * (1.0 + cha(c, d))).round() as u64
}
