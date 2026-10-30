//! Match statistics: Elo, normalized Elo and the generalized SPRT on game pairs (pentanomial), the way fastchess,
//! OpenBench and Fishtest compute them, so results here read the same as results there.
//!
//! References: Michel Van den Bergh, "Comments on normalized Elo" (normalized_elo_practical.pdf) and "Comparing the
//! approximations for the generalized log likelihood ratio" (comparing_approximations.pdf). The formulas follow
//! fastchess's `sprt.cpp` and `elo_pentanomial.cpp`, and the tests use its reference values.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::version::Estimate;

/// Game-pair outcomes from the dev engine's side: `[LL, LD, WL+DD, WD, WW]`, i.e. pairs scoring 0, ½, 1, 1½ and 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Pentanomial(pub [u64; 5]);

const SCORES: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
const NELO_SCALE: f64 = 800.0 / std::f64::consts::LN_10;
const Z95: f64 = 1.959963984540054;
/// The closest a score may come to 0 or 1 when it is turned into logistic Elo.
const SCORE_EPS: f64 = 1e-6;

impl Pentanomial {
    pub fn pairs(&self) -> u64 {
        self.0.iter().sum()
    }

    /// Games won minus games lost: pairs scoring 0, ½, 1, 1½, 2 are −2, −1, 0, +1, +2. SPSA steps by it.
    pub fn wins_minus_losses(&self) -> i64 {
        let [ll, ld, _, wd, ww] = self.0.map(|n| n as i64);
        -2 * ll - ld + wd + 2 * ww
    }

    /// The counts added up.
    pub fn add(&self, other: &Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + other.0[i]))
    }

    /// Reads fastchess's summary line, `Ptnml(0-2): [LL, LD, WL+DD, WD, WW], WL/DD Ratio: ...`.
    pub fn parse_fastchess(line: &str) -> Option<Self> {
        let inner = line.trim().strip_prefix("Ptnml(0-2): [")?.split(']').next()?;
        let mut counts = [0u64; 5];
        let mut parts = inner.split(',');
        for c in &mut counts {
            *c = parts.next()?.trim().parse().ok()?;
        }
        Some(Self(counts))
    }

    fn probs(&self, regularize: bool) -> ([f64; 5], f64) {
        let counts = self.0.map(|c| if regularize && c == 0 { 1e-3 } else { c as f64 });
        let total: f64 = counts.iter().sum();
        (counts.map(|c| c / total), total)
    }
}

/// The SPRT a test runs: Elo bounds in normalized Elo, and the error rates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Sprt {
    pub elo0: f64,
    pub elo1: f64,
    pub alpha: f64,
    pub beta: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// H1: the change is at least `elo1`.
    Accepted,
    /// H0: the change is at most `elo0`.
    Rejected,
    Continue,
}

impl Sprt {
    pub fn lower(&self) -> f64 {
        (self.beta / (1.0 - self.alpha)).ln()
    }

    pub fn upper(&self) -> f64 {
        ((1.0 - self.beta) / self.alpha).ln()
    }

    /// The log-likelihood ratio of the results, with the normalized-Elo model on game pairs.
    pub fn llr(&self, p: &Pentanomial) -> f64 {
        if p.pairs() == 0 {
            return 0.0;
        }
        let (probs, total) = p.probs(true);
        let t0 = std::f64::consts::SQRT_2 * self.elo0 / NELO_SCALE;
        let t1 = std::f64::consts::SQRT_2 * self.elo1 / NELO_SCALE;
        let p0 = mle_normalized(&probs, 0.5, t0);
        let p1 = mle_normalized(&probs, 0.5, t1);
        total * (0..5).map(|i| probs[i] * (p1[i].ln() - p0[i].ln())).sum::<f64>()
    }

    /// The decision at this LLR. fastchess stops a match when its own LLR crosses a bound, and it finds the MLE with a
    /// coarser root finder (ITP to 1e-7) than [`solve`] here, so the two can differ in the last digits: a match it
    /// stopped right at a bound must read as decided here too, not as undecided (which fails the stage).
    pub fn verdict(&self, llr: f64) -> Verdict {
        const SLACK: f64 = 1e-3;
        if llr >= self.upper() - SLACK {
            Verdict::Accepted
        } else if llr <= self.lower() + SLACK {
            Verdict::Rejected
        } else {
            Verdict::Continue
        }
    }
}

/// The maximum-likelihood distribution with `t = (mu - mu_ref) / sigma = t_star`, near the empirical one (section 4.1
/// of the normalized Elo paper), found iteratively from a uniform start.
fn mle_normalized(probs: &[f64; 5], mu_ref: f64, t_star: f64) -> [f64; 5] {
    let mut p = [0.2; 5];
    for _ in 0..10 {
        let (mu, var) = mean_var(&SCORES, &p);
        let sigma = var.sqrt();
        let phi: [f64; 5] = std::array::from_fn(|i| {
            let a = SCORES[i];
            a - mu_ref - 0.5 * t_star * sigma * (1.0 + ((a - mu) / sigma).powi(2))
        });
        let u = phi.iter().copied().fold(f64::INFINITY, f64::min);
        let v = phi.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let theta = solve(-1.0 / v, -1.0 / u, |x| (0..5).map(|i| probs[i] * phi[i] / (1.0 + x * phi[i])).sum());
        let mut diff = 0.0f64;
        for i in 0..5 {
            let next = probs[i] / (1.0 + theta * phi[i]);
            diff = diff.max((next - p[i]).abs());
            p[i] = next;
        }
        if diff < 1e-4 {
            break;
        }
    }
    p
}

/// The root of a decreasing function on the open interval (a, b).
fn solve(mut a: f64, mut b: f64, f: impl Fn(f64) -> f64) -> f64 {
    for _ in 0..200 {
        let mid = (a + b) / 2.0;
        if f(mid) > 0.0 {
            a = mid;
        } else {
            b = mid;
        }
        if b - a < 1e-12 {
            break;
        }
    }
    (a + b) / 2.0
}

fn mean_var(x: &[f64; 5], p: &[f64; 5]) -> (f64, f64) {
    let mu: f64 = (0..5).map(|i| x[i] * p[i]).sum();
    let var = (0..5).map(|i| p[i] * (x[i] - mu).powi(2)).sum();
    (mu, var)
}

/// Elo and normalized Elo with 95% intervals (`±`), from game pairs.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EloEstimate {
    pub elo: f64,
    pub elo_error: f64,
    pub nelo: f64,
    pub nelo_error: f64,
    /// Likelihood of superiority, 0 to 1.
    pub los: f64,
    /// Average points per game, 0 to 1.
    pub score: f64,
}

impl EloEstimate {
    /// Logistic Elo as the interval the API carries.
    pub fn elo_interval(&self) -> Estimate {
        Estimate::plus_minus(self.elo, self.elo_error)
    }

    /// Normalized Elo as the interval the API carries.
    pub fn nelo_interval(&self) -> Estimate {
        Estimate::plus_minus(self.nelo, self.nelo_error)
    }
}

/// Logistic Elo and normalized Elo with their 95% intervals, from game pairs.
///
/// Every field is finite, so it always serializes as a number (JSON has no NaN or infinity; serde would write `null`).
/// Two cases need care, both common in a test's first few pairs:
/// - Every pair scored the same (all draws, say): the variance is zero and normalized Elo would be 0/0. The spread is
///   then taken from the counts regularized as for the LLR ([`Sprt::llr`]): tiny, but not none.
/// - A score of 0 or 1, or an interval reaching past them: logistic Elo is infinite there, so scores are clamped to
///   [`SCORE_EPS`, 1 − `SCORE_EPS`] (about ±2,400 Elo) before converting.
pub fn estimate(p: &Pentanomial) -> Option<EloEstimate> {
    if p.pairs() == 0 {
        return None;
    }
    let (probs, pairs) = p.probs(false);
    let (score, mut var) = mean_var(&SCORES, &probs);
    if var <= 0.0 {
        var = mean_var(&SCORES, &p.probs(true).0).1;
    }
    let per_pair = (var / pairs).sqrt();
    let (hi, lo) = (score + Z95 * per_pair, score - Z95 * per_pair);
    let elo = |s: f64| {
        let s = s.clamp(SCORE_EPS, 1.0 - SCORE_EPS);
        -400.0 * (1.0 / s - 1.0).log10()
    };
    let nelo = |s: f64| (s - 0.5) / (2.0 * var).sqrt() * NELO_SCALE;
    let los = if per_pair > 0.0 { (1.0 - erf(-(score - 0.5) / (2.0f64.sqrt() * per_pair))) / 2.0 } else { 0.5 };
    Some(EloEstimate {
        elo: elo(score),
        elo_error: (elo(hi) - elo(lo)) / 2.0,
        nelo: nelo(score),
        nelo_error: (nelo(hi) - nelo(lo)) / 2.0,
        los,
        score,
    })
}

/// The error function (Abramowitz and Stegun 7.1.26, error under 1.5e-7), for the likelihood of superiority.
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592)
            * t
            * (-x * x).exp();
    y.copysign(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprt(elo0: f64, elo1: f64) -> Sprt {
        Sprt { elo0, elo1, alpha: 0.05, beta: 0.05 }
    }

    /// fastchess's constructor order is (LL, LD, WL, DD, WD, WW); WL and DD are one cell here.
    fn penta(ll: u64, ld: u64, wl: u64, dd: u64, wd: u64, ww: u64) -> Pentanomial {
        Pentanomial([ll, ld, wl + dd, wd, ww])
    }

    #[test]
    fn llr_matches_fastchess() {
        let close = |a: f64, b: f64| (a - b).abs() / b.abs() < 0.01;
        let llr = sprt(0.0, 2.0).llr(&penta(365, 16618, 36029, 200, 16974, 390));
        assert!(close(llr, 2.25), "{llr}");
        let llr = sprt(-1.75, 0.25).llr(&penta(127, 4883, 10311, 401, 5150, 104));
        assert!(close(llr, 3.01), "{llr}");
        let llr = sprt(0.0, 5.0).llr(&penta(0, 0, 0, 0, 0, 5550));
        assert!(close(llr, 111.82), "{llr}");
    }

    #[test]
    fn the_error_rates_set_the_llr_bounds() {
        // alpha 0.05, beta 0.20: the single-machine funnels'.
        let s = Sprt { elo0: 0.0, elo1: 4.0, alpha: 0.05, beta: 0.20 };
        assert!((s.lower() - (0.2f64 / 0.95).ln()).abs() < 1e-12, "{}", s.lower());
        assert!((s.upper() - (0.8f64 / 0.05).ln()).abs() < 1e-12, "{}", s.upper());
        assert!((s.lower() - -1.558).abs() < 1e-3 && (s.upper() - 2.773).abs() < 1e-3);
        // alpha 0.05, beta 0.10: Reckless's.
        let r = Sprt { beta: 0.10, ..s };
        assert!((r.lower() - (0.1f64 / 0.95).ln()).abs() < 1e-12 && (r.upper() - 18f64.ln()).abs() < 1e-12);
        // A larger beta stops sooner on the way down; a smaller alpha asks more on the way up.
        let strict = Sprt { alpha: 0.025, ..s };
        assert!(strict.upper() > s.upper() && s.lower() > r.lower());
        // The LLR itself does not depend on the rates, only where it stops.
        let p = penta(10, 300, 900, 20, 320, 12);
        assert_eq!(s.llr(&p), r.llr(&p));
    }

    #[test]
    fn bounds_and_verdicts() {
        let s = Sprt { elo0: 0.0, elo1: 3.0, alpha: 0.05, beta: 0.10 };
        assert!((s.lower() - -2.25).abs() < 0.01 && (s.upper() - 2.89).abs() < 0.01);
        assert_eq!(s.verdict(3.0), Verdict::Accepted);
        assert_eq!(s.verdict(-3.0), Verdict::Rejected);
        assert_eq!(s.verdict(0.0), Verdict::Continue);
        // Right at a bound, as fastchess stops: decided, whatever the last digits.
        assert_eq!(s.verdict(s.upper() - 1e-6), Verdict::Accepted);
        assert_eq!(s.verdict(s.lower() + 1e-6), Verdict::Rejected);
        assert_eq!(s.verdict(s.upper() - 0.01), Verdict::Continue);
    }

    #[test]
    fn estimates_an_even_match_as_zero() {
        let e = estimate(&Pentanomial([100, 400, 1000, 400, 100])).unwrap();
        assert!(e.elo.abs() < 1e-9 && e.nelo.abs() < 1e-9);
        assert!(e.elo_error > 0.0 && (e.los - 0.5).abs() < 1e-6);
        let e = estimate(&Pentanomial([50, 300, 1000, 500, 150])).unwrap();
        assert!(e.elo > 0.0 && e.los > 0.99);
    }

    #[test]
    fn pairs_count_wins_minus_losses_and_read_fastchess() {
        assert_eq!(Pentanomial([1, 2, 3, 4, 5]).wins_minus_losses(), 10); // -2 - 2 + 4 + 10
        assert_eq!(Pentanomial([0, 0, 7, 0, 0]).wins_minus_losses(), 0);
        assert_eq!(
            Pentanomial::parse_fastchess("Ptnml(0-2): [1, 1, 3, 1, 2], WL/DD Ratio: 2.00"),
            Some(Pentanomial([1, 1, 3, 1, 2]))
        );
        assert_eq!(Pentanomial::parse_fastchess("LLR: 0.02 (0.6%) (-2.25, 2.89) [0.00, 3.00]"), None);
        assert_eq!(Pentanomial([1, 0, 0, 0, 1]).add(&Pentanomial([0, 1, 0, 1, 0])), Pentanomial([1, 1, 0, 1, 1]));
        let e = estimate(&Pentanomial([50, 300, 1000, 500, 150])).unwrap();
        assert!((e.elo_interval().error() - e.elo_error).abs() < 1e-9);
    }

    #[test]
    fn estimates_stay_finite_without_spread() {
        let finite = |e: EloEstimate| {
            let all = [e.elo, e.elo_error, e.nelo, e.nelo_error, e.los, e.score];
            assert!(all.iter().all(|x| x.is_finite()), "{e:?}");
            e
        };
        // All draws: no variance, an even score.
        let e = finite(estimate(&Pentanomial([0, 0, 7, 0, 0])).unwrap());
        assert!(e.elo.abs() < 1e-9 && e.nelo.abs() < 1e-9 && e.elo_error > 0.0);
        // A clean sweep each way.
        let e = finite(estimate(&Pentanomial([0, 0, 0, 0, 3])).unwrap());
        assert!(e.elo > 1000.0 && e.nelo > 0.0 && e.score == 1.0);
        let e = finite(estimate(&Pentanomial([3, 0, 0, 0, 0])).unwrap());
        assert!(e.elo < -1000.0 && e.nelo < 0.0);
        // An interval reaching past a score of 1.
        finite(estimate(&Pentanomial([0, 0, 1, 0, 9])).unwrap());
        let json = serde_json::to_string(&estimate(&Pentanomial([0, 0, 0, 0, 3])).unwrap()).unwrap();
        assert!(!json.contains("null"), "{json}");
    }
}
