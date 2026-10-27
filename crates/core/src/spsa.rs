//! SPSA tuning, as OpenBench does it: a build's declared tunables (Reckless's `define!` macro, built with the `spsa`
//! feature) are moved by simultaneous perturbation. Each iteration plays a few game pairs between the parameters
//! nudged one way and the other, and steps them toward the side that scored better.
//!
//! OpenBench (`OpenBench/spsa_utils.py`: `create_spsa_run` and `spsa_workload_assignment_dict`; the client's
//! `report_results` for the step), with `N` iterations, `A = A_ratio · N` and each parameter's `c_end` and `r_end`:
//!
//! ```text
//! c       = c_end · N^γ                      a       = r_end · c_end² · (A + N)^α
//! c_k     = max(c / k^γ, 0.5)                a_k     = a / (A + k)^α           (k = 1, 2, … N)
//! R_k     = a_k / c_k²
//! θ±      = clamp(floor(θ ± c_k · Δ + u)),   Δ = ±1 per parameter at random, u uniform in [0, 1), one per parameter
//! θ      ← clamp(θ + R_k · c_k · (W − L) · Δ)
//! ```
//!
//! where `W − L` counts games won minus games lost by θ+ against θ−. So `c_k` shrinks to `c_end` and `R_k` to `r_end`
//! at the last iteration, and the step is the usual SPSA gradient estimate `a_k · (W − L) / c_k` along Δ.
//!
//! The rounding is OpenBench's stochastic rounding, with one `u` shared by both sides: on average θ± is exactly
//! θ ± c_k, and the two sides always differ by `2·c_k` rounded down or up, never by zero while `c_k ≥ 0.5` (which the
//! floor on `c_k`, OpenBench's for integer tunables, guarantees).
//!
//! ## Filling the machine
//!
//! An iteration is small (8 pairs by default), so a tune runs like OpenBench's fleet: its slots are split into workers
//! of two games ([`workers`]), and each worker plays one iteration's batch. A batch is drawn when a worker frees up —
//! its own Δ, θ± around θ as it stands then, and `c_k`, `R_k` for `k` = updates applied so far + 1 — and its step is
//! added to θ as it stands when the batch finishes ([`Scheduler`]). Several batches are in flight at once, each a few
//! updates behind, as with OpenBench's many workers; every update is one stored iteration, numbered in the order
//! applied.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{ideas::Preset, stats::Pentanomial, time::TimeControl};

/// OpenBench's defaults.
pub const ALPHA: f64 = 0.602;
pub const GAMMA: f64 = 0.101;
pub const A_RATIO: f64 = 0.1;
pub const R_END: f64 = 0.002;
/// Iterations of a tune when none are asked for.
pub const DEFAULT_ITERATIONS: u32 = 1000;
/// Game pairs per iteration, as OpenBench batches SPSA.
pub const DEFAULT_PAIRS: u32 = 8;

/// The smallest perturbation of an integer tunable, as OpenBench floors it.
pub const MIN_C: f64 = 0.5;

/// One tunable: its range and step sizes, and where it starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Tunable {
    pub name: String,
    pub start: f64,
    pub min: f64,
    pub max: f64,
    /// The perturbation at the last iteration.
    pub c_end: f64,
    /// The learning rate `a_k / c_k²` at the last iteration.
    pub r_end: f64,
}

/// A parameter to tune, as asked: anything left out comes from the engine's declaration, so `{ "name": "x" }` tunes
/// `x` as declared.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TunableOverride {
    pub name: String,
    pub start: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub c_end: Option<f64>,
    pub r_end: Option<f64>,
}

/// What a tune idea runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TuneSpec {
    /// The parameters; empty means every declared tunable.
    #[serde(default)]
    pub params: Vec<TunableOverride>,
    pub iterations: u32,
    /// Game pairs per iteration.
    pub pairs: u32,
    /// The games' clock as written (scaled when the tune starts), or a node limit.
    pub time: TimeControl,
    /// Hash in MiB.
    pub hash: u32,
    /// Games at once; none for what the scheduler grants.
    pub concurrency: Option<u32>,
    pub alpha: f64,
    pub gamma: f64,
    pub a_ratio: f64,
}

impl Default for TuneSpec {
    /// OpenBench's defaults at STC.
    fn default() -> Self {
        let stc = Preset::Stc.settings();
        Self {
            params: Vec::new(),
            iterations: DEFAULT_ITERATIONS,
            pairs: DEFAULT_PAIRS,
            time: stc.time,
            hash: stc.hash,
            concurrency: None,
            alpha: ALPHA,
            gamma: GAMMA,
            a_ratio: A_RATIO,
        }
    }
}

impl TuneSpec {
    pub fn schedule(&self) -> Schedule {
        Schedule { iterations: self.iterations, alpha: self.alpha, gamma: self.gamma, a_ratio: self.a_ratio }
    }
}

/// A parameter being tuned, with where it is now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TuneParam {
    #[serde(flatten)]
    pub tunable: Tunable,
    /// The current value, unrounded.
    pub value: f64,
}

impl TuneParam {
    pub fn start(tunable: Tunable) -> Self {
        Self { value: tunable.start, tunable }
    }
}

/// One applied update: the store's `tune_steps` row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TuneStep {
    /// Counted from 1, in the order updates were applied.
    pub iteration: u32,
    /// Every parameter after this step, in the order of the tune's parameters.
    pub values: Vec<f64>,
    /// Games won minus games lost by θ+.
    pub result: i64,
    /// θ+ against θ−, as game pairs from θ+'s side.
    pub penta: Pentanomial,
}

/// The schedule of a tune: how `c_k` and `R_k` shrink over its iterations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Schedule {
    pub iterations: u32,
    pub alpha: f64,
    pub gamma: f64,
    pub a_ratio: f64,
}

impl Schedule {
    fn big_a(&self) -> f64 {
        self.a_ratio * self.iterations as f64
    }

    /// `c_k` for `p` at iteration `k`, counted from 1, never under [`MIN_C`] (every tunable is an integer).
    pub fn c(&self, p: &Tunable, k: u32) -> f64 {
        let c = p.c_end * (self.iterations as f64).powf(self.gamma);
        (c / (k as f64).powf(self.gamma)).max(MIN_C)
    }

    /// `R_k = a_k / c_k²` for `p` at iteration `k`, counted from 1.
    pub fn r(&self, p: &Tunable, k: u32) -> f64 {
        let a = p.r_end * p.c_end * p.c_end * (self.big_a() + self.iterations as f64).powf(self.alpha);
        let a_k = a / (self.big_a() + k as f64).powf(self.alpha);
        a_k / self.c(p, k).powi(2)
    }
}

/// The values the two engines play with: θ + c·Δ and θ − c·Δ, rounded stochastically with `u` in [0, 1) (shared by
/// both sides, as OpenBench does) and then clamped (every tunable is an integer).
pub fn perturb(p: &Tunable, value: f64, c: f64, flip: f64, u: f64) -> (f64, f64) {
    let side = |v: f64| (v + u).floor().clamp(p.min, p.max);
    (side(value + c * flip), side(value - c * flip))
}

/// The new value after an iteration whose θ+ side won `result` more games than it lost.
pub fn step(p: &Tunable, value: f64, r: f64, c: f64, flip: f64, result: i64) -> f64 {
    (value + r * c * result as f64 * flip).clamp(p.min, p.max)
}

/// The tune's game slots as workers: two each (a game pair at a time, both games at once), and one left over when
/// the concurrency is odd.
pub fn workers(concurrency: u32) -> Vec<u32> {
    let mut slots = vec![2; (concurrency / 2) as usize];
    if concurrency % 2 == 1 || slots.is_empty() {
        slots.push(1);
    }
    slots
}

/// The randomness of one batch: a ±1 flip per parameter, and the `u` in [0, 1) its θ± are rounded with.
#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    pub flips: Vec<f64>,
    pub rounding: Vec<f64>,
}

impl Draw {
    /// A draw from random 64-bit words, one per parameter (the caller draws them): the top bit for the flip, the low
    /// 53 bits for `u`.
    pub fn from_words(words: &[u64]) -> Self {
        Self {
            flips: words.iter().map(|w| if w >> 63 == 1 { 1.0 } else { -1.0 }).collect(),
            rounding: words.iter().map(|w| (w & ((1 << 53) - 1)) as f64 / (1u64 << 53) as f64).collect(),
        }
    }
}

/// One iteration's batch of games: θ± as drawn at dispatch, and what its result needs to become a step.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    /// The `k` its `c_k` and `R_k` belong to.
    pub k: u32,
    pub flips: Vec<f64>,
    /// `(c_k, R_k)` per parameter.
    pub steps: Vec<(f64, f64)>,
    /// θ+ and θ− as UCI options.
    pub plus: Vec<(String, String)>,
    pub minus: Vec<(String, String)>,
}

/// Draws batches against the current θ and applies their results to θ as it stands when they come back, as
/// OpenBench's server does for its workers.
#[derive(Clone, Debug)]
pub struct Scheduler {
    schedule: Schedule,
    pub params: Vec<TuneParam>,
    /// Updates applied, the tune's iteration.
    applied: u32,
    /// Batches drawn and not yet applied.
    in_flight: u32,
}

impl Scheduler {
    /// A scheduler at θ = the params' values, with `applied` updates already made (a resumed tune).
    pub fn new(schedule: Schedule, params: Vec<TuneParam>, applied: u32) -> Self {
        Self { schedule, params, applied, in_flight: 0 }
    }

    pub fn applied(&self) -> u32 {
        self.applied
    }

    /// The next batch, drawn with this randomness around the current θ; `None` once every iteration left is drawn.
    pub fn next(&mut self, draw: Draw) -> Option<Batch> {
        if self.applied + self.in_flight >= self.schedule.iterations {
            return None;
        }
        self.in_flight += 1;
        let k = self.applied + 1;
        let Draw { flips, rounding } = draw;
        let mut batch = Batch { k, flips, steps: Vec::new(), plus: Vec::new(), minus: Vec::new() };
        for ((p, &flip), &u) in self.params.iter().zip(&batch.flips).zip(&rounding) {
            let (c, rk) = (self.schedule.c(&p.tunable, k), self.schedule.r(&p.tunable, k));
            let (hi, lo) = perturb(&p.tunable, p.value, c, flip, u);
            batch.plus.push((p.tunable.name.clone(), format!("{hi}")));
            batch.minus.push((p.tunable.name.clone(), format!("{lo}")));
            batch.steps.push((c, rk));
        }
        Some(batch)
    }

    /// Steps θ by a finished batch's result (θ+'s wins minus losses) and returns the iteration this update is.
    pub fn apply(&mut self, batch: &Batch, result: i64) -> u32 {
        for ((p, &flip), &(c, rk)) in self.params.iter_mut().zip(&batch.flips).zip(&batch.steps) {
            p.value = step(&p.tunable, p.value, rk, c, flip, result);
        }
        self.in_flight = self.in_flight.saturating_sub(1);
        self.applied += 1;
        self.applied
    }

    /// A batch that will not come back (its worker failed): it is drawn again later.
    pub fn abandon(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    /// θ now, in the params' order.
    pub fn values(&self) -> Vec<f64> {
        self.params.iter().map(|p| p.value).collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// What an engine declares
// ---------------------------------------------------------------------------------------------------------------------

/// The tunables in `reckless spsa`'s output, OpenBench's format: lines like `rfp_quad, int, 1140, 600, 1700, 55, 0.002`.
pub fn parse_openbench(out: &str) -> Vec<Tunable> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            let [name, kind, start, min, max, c_end, r_end] = f.as_slice() else { return None };
            if *kind != "int" {
                return None;
            }
            Some(Tunable {
                name: (*name).to_owned(),
                start: start.parse().ok()?,
                min: min.parse().ok()?,
                max: max.parse().ok()?,
                c_end: c_end.parse().ok()?,
                r_end: r_end.parse().ok()?,
            })
        })
        .collect()
}

/// The engine options that are not tunables.
const STANDARD_OPTIONS: &[&str] = &["Hash", "Threads", "MoveOverhead", "MultiPV"];

/// The tunables from a `uci` answer, when the build has no `spsa` command: every spin option that is not a standard
/// one, with OpenBench's usual steps (`c_end` a twentieth of the range, at least 0.5).
pub fn parse_uci_options(out: &str) -> Vec<Tunable> {
    out.lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            let field = |key: &str| w.iter().position(|x| *x == key).and_then(|i| w.get(i + 1)?.parse::<f64>().ok());
            if w.first() != Some(&"option") || !l.contains("type spin") {
                return None;
            }
            let name = w.get(2)?;
            if STANDARD_OPTIONS.contains(name) {
                return None;
            }
            let (start, min, max) = (field("default")?, field("min")?, field("max")?);
            Some(Tunable {
                name: (*name).to_owned(),
                start,
                min,
                max,
                c_end: ((max - min) / 20.0).max(0.5),
                r_end: R_END,
            })
        })
        .collect()
}

/// The parameters to tune: every declared one when `overrides` is empty, else those named, with the overrides given.
pub fn select(declared: &[Tunable], overrides: &[TunableOverride]) -> Result<Vec<Tunable>, String> {
    if overrides.is_empty() {
        return Ok(declared.to_vec());
    }
    overrides
        .iter()
        .map(|i| {
            let d = declared.iter().find(|d| d.name == i.name);
            let field = |given: Option<f64>, from: fn(&Tunable) -> f64, what: &str| {
                given
                    .or(d.map(from))
                    .ok_or_else(|| format!("{} is not declared by the engine: give its {what}", i.name))
            };
            let p = Tunable {
                name: i.name.clone(),
                start: field(i.start, |d| d.start, "start")?,
                min: field(i.min, |d| d.min, "min")?,
                max: field(i.max, |d| d.max, "max")?,
                c_end: field(i.c_end, |d| d.c_end, "c_end")?,
                r_end: i.r_end.or(d.map(|d| d.r_end)).unwrap_or(R_END),
            };
            if !(p.min <= p.start && p.start <= p.max) || p.c_end <= 0.0 {
                return Err(format!("{}: needs min ≤ start ≤ max and c_end > 0", p.name));
            }
            Ok(p)
        })
        .collect()
}

/// Replaces each parameter's default in `src/parameters.rs`'s `define!` block (`i32 name: default, …;` or
/// `i32 name: default;`) with its tuned value, rounded unless the default is written with a decimal point. Returns
/// the new source and one line per changed parameter.
pub fn rewrite_defaults(source: &str, params: &[TuneParam]) -> Result<(String, Vec<String>), String> {
    let mut source = source.to_owned();
    let mut changes = Vec::new();
    for p in params {
        let name = &p.tunable.name;
        let re = regex_lite::Regex::new(&format!(
            r"(?m)^(\s*[A-Za-z0-9_]+\s+{}\s*:\s*)(-?[0-9][0-9_.]*)",
            regex_lite::escape(name)
        ))
        .map_err(|e| e.to_string())?;
        let mut found = re.captures_iter(&source);
        let (Some(caps), None) = (found.next(), found.next()) else {
            return Err(format!("{name} is not declared exactly once in src/parameters.rs"));
        };
        let Some(whole) = caps.get(0).map(|m| m.range()) else { continue };
        let (prefix, old) = (caps[1].to_owned(), caps[2].to_owned());
        let new = if old.contains('.') { format!("{:.4}", p.value) } else { format!("{}", p.value.round() as i64) };
        if new != old {
            changes.push(format!("{name}: {old} -> {new}"));
            source.replace_range(whole, &format!("{prefix}{new}"));
        }
    }
    Ok((source, changes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param() -> Tunable {
        Tunable { name: "x".into(), start: 100.0, min: 50.0, max: 150.0, c_end: 5.0, r_end: 0.002 }
    }

    fn schedule() -> Schedule {
        Schedule { iterations: 1000, alpha: ALPHA, gamma: GAMMA, a_ratio: A_RATIO }
    }

    #[test]
    fn ends_at_c_end_and_r_end() {
        let (p, s) = (param(), schedule());
        assert!((s.c(&p, 1000) - 5.0).abs() < 1e-9);
        assert!((s.r(&p, 1000) - 0.002).abs() < 1e-12);
    }

    #[test]
    fn starts_wider_and_shrinks() {
        let (p, s) = (param(), schedule());
        // c_1 = c_end · N^γ = 5 · 1000^0.101.
        assert!((s.c(&p, 1) - 5.0 * 1000f64.powf(0.101)).abs() < 1e-9);
        assert!(s.c(&p, 1) > s.c(&p, 500) && s.c(&p, 500) > s.c(&p, 1000));
        // a_1 / a_N = ((A + N) / (A + 1))^α with A = 100.
        let a = |k| s.r(&p, k) * s.c(&p, k).powi(2);
        assert!((a(1) / a(1000) - (1100f64 / 101.0).powf(0.602)).abs() < 1e-9);
    }

    #[test]
    fn perturbs_both_ways_within_bounds() {
        let p = param();
        assert_eq!(perturb(&p, 100.0, 6.4, 1.0, 0.5), (106.0, 94.0));
        assert_eq!(perturb(&p, 100.0, 6.4, -1.0, 0.5), (94.0, 106.0));
        assert_eq!(perturb(&p, 148.0, 6.4, 1.0, 0.5), (150.0, 142.0));
        // Rounded with u: 106.4 and 93.6 go down below u = 0.6 and up from it.
        assert_eq!(perturb(&p, 100.0, 6.4, 1.0, 0.0), (106.0, 93.0));
        assert_eq!(perturb(&p, 100.0, 6.4, 1.0, 0.99), (107.0, 94.0));
    }

    #[test]
    fn stochastic_rounding_is_unbiased_and_never_collapses() {
        let p = param();
        // θ = 100.3, c = 0.5: the sides are 100.8 and 99.8 on average, and always one apart.
        let n = 1000;
        let (mut hi, mut lo) = (0.0, 0.0);
        for i in 0..n {
            let (h, l) = perturb(&p, 100.3, 0.5, 1.0, (i as f64 + 0.5) / n as f64);
            assert_eq!(h - l, 1.0);
            (hi, lo) = (hi + h, lo + l);
        }
        assert!((hi / n as f64 - 100.8).abs() < 1e-3 && (lo / n as f64 - 99.8).abs() < 1e-3);
    }

    #[test]
    fn floors_c_at_a_half() {
        let p = Tunable { c_end: 0.2, ..param() };
        let s = schedule();
        assert_eq!(s.c(&p, 1000), MIN_C);
        // R_k uses the floored c_k, as OpenBench's does.
        let a = p.r_end * p.c_end * p.c_end;
        assert!((s.r(&p, 1000) - a / (MIN_C * MIN_C)).abs() < 1e-12);
    }

    #[test]
    fn steps_toward_the_winning_side() {
        let p = param();
        // θ+ was θ + c (flip +1) and won 4 more games: θ moves up by R·c·4.
        assert!((step(&p, 100.0, 0.01, 6.0, 1.0, 4) - 100.24).abs() < 1e-9);
        // θ+ was θ − c (flip −1) and won: θ moves down.
        assert!((step(&p, 100.0, 0.01, 6.0, -1.0, 4) - 99.76).abs() < 1e-9);
        assert_eq!(step(&p, 149.9, 1.0, 6.0, 1.0, 10), 150.0);
    }

    #[test]
    fn splits_the_slots_into_workers_of_two() {
        assert_eq!(workers(14), [2; 7]);
        assert_eq!(workers(5), [2, 2, 1]);
        assert_eq!(workers(1), [1]);
        assert_eq!(workers(0), [1]);
    }

    fn scheduler(iterations: u32, applied: u32) -> Scheduler {
        let tunable = Tunable { name: "x".into(), start: 100.0, min: 0.0, max: 200.0, c_end: 4.0, r_end: 0.002 };
        let schedule = Schedule { iterations, alpha: ALPHA, gamma: GAMMA, a_ratio: A_RATIO };
        Scheduler::new(schedule, vec![TuneParam::start(tunable)], applied)
    }

    /// One parameter's draw with this flip, rounded half up.
    fn fixed(flip: f64) -> Draw {
        Draw { flips: vec![flip], rounding: vec![0.5] }
    }

    #[test]
    fn draws_flips_and_rounding_in_range() {
        let words: Vec<u64> = (0..64u64).map(|i| i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).collect();
        let d = Draw::from_words(&words);
        assert!(d.flips.iter().all(|f| *f == 1.0 || *f == -1.0));
        assert!(d.rounding.iter().all(|u| (0.0..1.0).contains(u)));
        assert!(d.flips.contains(&1.0) && d.flips.contains(&-1.0));
        assert_eq!(Draw::from_words(&[u64::MAX]).flips, [1.0]);
        assert_eq!(Draw::from_words(&[0]).rounding, [0.0]);
    }

    #[test]
    fn batches_in_flight_share_theta_and_k_until_a_result_lands() {
        let mut s = scheduler(10, 0);
        let a = s.next(fixed(1.0)).unwrap();
        let b = s.next(fixed(-1.0)).unwrap();
        // Both drawn against θ = 100 at k = 1, with opposite flips.
        assert_eq!((a.k, b.k), (1, 1));
        assert_eq!(a.plus, b.minus);
        assert_eq!(a.minus, b.plus);

        // b lands first: θ moves by b's own step, and becomes iteration 1.
        let (c, rk) = b.steps[0];
        assert_eq!(s.apply(&b, 4), 1);
        let after_b = 100.0 - rk * c * 4.0;
        assert!((s.params[0].value - after_b).abs() < 1e-9);

        // A batch drawn now sees the moved θ and k = 2.
        let c2 = s.next(fixed(1.0)).unwrap();
        assert_eq!(c2.k, 2);
        let (cc, _) = c2.steps[0];
        assert_eq!(c2.plus[0].1, format!("{}", (after_b + cc).round()));

        // a lands next, applied to θ as it stands (not to the θ it was drawn at), with a's c and R, as iteration 2.
        let (ca, ra) = a.steps[0];
        assert_eq!(s.apply(&a, -2), 2);
        assert!((s.params[0].value - (after_b - ra * ca * 2.0)).abs() < 1e-9);
    }

    #[test]
    fn draws_exactly_the_iterations_left() {
        // A resumed tune with 7 of 10 applied: three batches, however many workers ask.
        let mut s = scheduler(10, 7);
        let drawn: Vec<Batch> = (0..7).filter_map(|_| s.next(fixed(1.0))).collect();
        assert_eq!(drawn.len(), 3);
        assert!(drawn.iter().all(|b| b.k == 8));
        let numbers: Vec<u32> = drawn.iter().map(|b| s.apply(b, 0)).collect();
        assert_eq!(numbers, [8, 9, 10]);
        assert!(s.next(fixed(1.0)).is_none());
        // An abandoned batch is drawn again.
        let mut s = scheduler(1, 0);
        assert!(s.next(fixed(1.0)).is_some());
        assert!(s.next(fixed(1.0)).is_none());
        s.abandon();
        assert!(s.next(fixed(1.0)).is_some());
    }

    #[test]
    fn reads_declared_tunables() {
        let ob = "asp_delta, int, 23, 10, 40, 1.5, 0.002\nrfp_quad, int, 1140, 600, 1700, 55, 0.002\n";
        let p = parse_openbench(ob);
        assert_eq!(p.len(), 2);
        assert_eq!((p[1].name.as_str(), p[1].start, p[1].c_end), ("rfp_quad", 1140.0, 55.0));

        let uci = "id name Reckless\noption name Hash type spin default 16 min 1 max 262144\n\
                   option name Minimal type check default false\n\
                   option name rfp_quad type spin default 1140 min 600 max 1700\nuciok\n";
        let p = parse_uci_options(uci);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].min, p[0].max, p[0].c_end), (600.0, 1700.0, 55.0));
    }

    #[test]
    fn selects_named_parameters_with_overrides() {
        let declared = parse_openbench("a, int, 10, 0, 20, 1, 0.002\nb, int, 5, 0, 9, 0.5, 0.002\n");
        assert_eq!(select(&declared, &[]).unwrap().len(), 2);
        let input = TunableOverride { name: "b".into(), max: Some(8.0), ..Default::default() };
        let p = select(&declared, &[input]).unwrap();
        assert_eq!((p.len(), p[0].max, p[0].start), (1, 8.0, 5.0));
        assert!(select(&declared, &[TunableOverride { name: "zzz".into(), ..Default::default() }]).is_err());
        let bad = TunableOverride { name: "a".into(), start: Some(30.0), ..Default::default() };
        assert!(select(&declared, &[bad]).is_err());
    }

    #[test]
    fn rewrites_defaults_in_place() {
        let src = "define! {\r\n    i32 asp_delta: 23, 10, 40, 1.5;\r\n    i32 rfp_quad: 1140, 600, 1700, 55.0;\r\n    f32 tm1: 3.1838;\r\n}\r\n";
        let tp = |name: &str, value| TuneParam {
            tunable: Tunable { name: name.into(), start: 0.0, min: 0.0, max: 0.0, c_end: 1.0, r_end: 0.002 },
            value,
        };
        let (out, changes) =
            rewrite_defaults(src, &[tp("asp_delta", 24.6), tp("rfp_quad", 1140.2), tp("tm1", 3.2)]).unwrap();
        assert!(out.contains("i32 asp_delta: 25, 10, 40, 1.5;\r\n"), "{out}");
        assert!(out.contains("i32 rfp_quad: 1140, 600"));
        assert!(out.contains("f32 tm1: 3.2000;"));
        assert_eq!(changes, ["asp_delta: 23 -> 25", "tm1: 3.1838 -> 3.2000"]);
        assert!(rewrite_defaults(src, &[tp("nope", 1.0)]).is_err());
    }

    #[test]
    fn specs_default_to_openbench_at_stc() {
        let s = TuneSpec::default();
        assert_eq!((s.iterations, s.pairs, s.hash), (1000, 8, 16));
        assert_eq!(s.time.seconds_text(), "8+0.08");
        assert_eq!(s.schedule().iterations, 1000);
        let json = serde_json::to_value(TuneParam::start(param())).unwrap();
        assert_eq!(json["name"], "x");
        assert_eq!(json["value"], 100.0);
    }
}
