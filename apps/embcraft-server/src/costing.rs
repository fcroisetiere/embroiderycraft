//! Sewing speed and cost estimate of a design, from the shop's cost settings.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// The shop's cost settings (Settings › Costs), kept in `meta` as JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Costing {
    /// Stitches per minute when a design has no speed of its own.
    pub default_speed: u32,
    /// Heads sewing at once (pieces per run).
    pub heads: u32,
    /// Seconds lost at each colour change.
    pub colour_change_seconds: f64,
    /// Seconds lost at each trim.
    pub trim_seconds: f64,
    /// Operator time per piece: hooping, unhooping, trimming threads.
    pub handling_seconds: f64,
    /// Machine and operator cost per hour.
    pub hourly_rate: f64,
    /// Thread and bobbin cost per 1000 stitches.
    pub thread_per_1000: f64,
    /// Backing and topping per piece.
    pub backing_per_piece: f64,
    /// Selling price per 1000 stitches.
    pub price_per_1000: f64,
    /// Lowest selling price per piece.
    pub minimum_price: f64,
    pub currency: String,
}

impl Default for Costing {
    fn default() -> Self {
        Self {
            default_speed: 800,
            heads: 1,
            colour_change_seconds: 5.0,
            trim_seconds: 4.0,
            handling_seconds: 60.0,
            hourly_rate: 45.0,
            thread_per_1000: 0.04,
            backing_per_piece: 0.15,
            price_per_1000: 1.0,
            minimum_price: 5.0,
            currency: "$".into(),
        }
    }
}

pub fn get(db: &Connection) -> rusqlite::Result<Costing> {
    let v: Option<String> = db
        .query_row("SELECT value FROM meta WHERE key = 'costing'", [], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(v.and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default())
}

pub fn set(db: &Connection, c: &Costing) -> Result<(), String> {
    if !(50..=3000).contains(&c.default_speed) {
        return Err("Speed: between 50 and 3000 stitches a minute.".into());
    }
    if !(1..=100).contains(&c.heads) {
        return Err("Heads: between 1 and 100.".into());
    }
    let numbers = [
        c.colour_change_seconds,
        c.trim_seconds,
        c.handling_seconds,
        c.hourly_rate,
        c.thread_per_1000,
        c.backing_per_piece,
        c.price_per_1000,
        c.minimum_price,
    ];
    if numbers.iter().any(|n| !n.is_finite() || *n < 0.0) {
        return Err("Costs and times are numbers, 0 or more.".into());
    }
    let json = serde_json::to_string(c).map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO meta (key, value) VALUES ('costing', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let has: bool = db.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('designs') WHERE name = 'speed'",
        [],
        |r| r.get(0),
    )?;
    if !has {
        db.execute_batch("ALTER TABLE designs ADD COLUMN speed INTEGER")?;
    }
    Ok(())
}

/// What one piece takes and costs.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Estimate {
    /// Stitches per minute used.
    pub speed: u32,
    /// Machine minutes for one run (all heads).
    pub sew_minutes: f64,
    /// Minutes per piece: the run shared by the heads, plus handling.
    pub minutes_per_piece: f64,
    pub cost: f64,
    pub price: f64,
    pub currency: String,
}

fn cents(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The estimate for a design with these counts, or `None` without stitches.
pub fn estimate(
    c: &Costing,
    speed: Option<u32>,
    stitches: Option<i64>,
    colours: Option<i64>,
    trims: Option<i64>,
) -> Option<Estimate> {
    let stitches = stitches.filter(|&s| s > 0)? as f64;
    let speed = speed.filter(|&s| s > 0).unwrap_or(c.default_speed).max(1);
    let changes = (colours.unwrap_or(1) - 1).max(0) as f64;
    let trims = trims.unwrap_or(0).max(0) as f64;
    let sew = stitches / speed as f64
        + (changes * c.colour_change_seconds + trims * c.trim_seconds) / 60.0;
    let per_piece = sew / c.heads.max(1) as f64 + c.handling_seconds / 60.0;
    let cost = per_piece / 60.0 * c.hourly_rate
        + stitches / 1000.0 * c.thread_per_1000
        + c.backing_per_piece;
    let price = (stitches / 1000.0 * c.price_per_1000).max(c.minimum_price);
    Some(Estimate {
        speed,
        sew_minutes: (sew * 10.0).round() / 10.0,
        minutes_per_piece: (per_piece * 10.0).round() / 10.0,
        cost: cents(cost),
        price: cents(price),
        currency: c.currency.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_belt_name() {
        let c = Costing::default();
        // 2,400 stitches, one colour, 2 trims, at 800 st/min: 3 min + 8 s sewing, 1 min handling
        let e = estimate(&c, None, Some(2400), Some(1), Some(2)).unwrap_or_else(|| panic!());
        assert_eq!(e.speed, 800);
        assert_eq!(e.sew_minutes, 3.1);
        assert_eq!(e.minutes_per_piece, 4.1);
        // 4.13 min at 45 $/h = 3.10, thread 0.10, backing 0.15
        assert_eq!(e.cost, 3.35);
        assert_eq!(e.price, 5.0);
        let six = Costing {
            heads: 6,
            ..Costing::default()
        };
        let e6 =
            estimate(&six, Some(1000), Some(24000), Some(4), Some(10)).unwrap_or_else(|| panic!());
        assert_eq!(e6.price, 24.0);
        assert!(e6.minutes_per_piece < e6.sew_minutes);
        assert_eq!(estimate(&c, None, None, None, None), None);
        let db =
            crate::db::open(std::path::Path::new(":memory:")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(get(&db).ok(), Some(Costing::default()));
        assert!(set(&db, &six).is_ok());
        assert_eq!(get(&db).map(|c| c.heads).ok(), Some(6));
        assert!(
            set(
                &db,
                &Costing {
                    hourly_rate: -1.0,
                    ..six
                }
            )
            .is_err()
        );
    }
}
