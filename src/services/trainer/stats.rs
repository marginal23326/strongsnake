/// Running statistics over pair scores. A pair is the independent sample, so the error bars treat it as one.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Tally {
    pub n: usize,
    sum: f64,
    sum_sq: f64,
    /// Games that hit the turn cap, where the winner is decided by length.
    pub capped: usize,
}

const Z95: f64 = 1.96;

/// Two-sided 95% critical value of Student's t (Cornish-Fisher expansion); the normal 1.96 is too narrow for few pairs.
fn t95(df: f64) -> f64 {
    let z = Z95;
    z + (z.powi(3) + z) / (4.0 * df) + (5.0 * z.powi(5) + 16.0 * z.powi(3) + 3.0 * z) / (96.0 * df * df)
}

impl Tally {
    pub fn add(&mut self, score: f64, capped: usize) {
        self.n += 1;
        self.sum += score;
        self.sum_sq += score * score;
        self.capped += capped;
    }

    /// Mean pair score; 0.5 when nothing was played.
    pub fn mean(&self) -> f64 {
        if self.n == 0 { 0.5 } else { self.sum / self.n as f64 }
    }

    /// Sample variance of one pair's score; 0 below two pairs.
    pub fn variance(&self) -> f64 {
        if self.n < 2 {
            return 0.0;
        }
        let n = self.n as f64;
        ((self.sum_sq - self.sum * self.sum / n) / (n - 1.0)).max(0.0)
    }

    /// Standard error of the mean; infinite below two pairs.
    pub fn std_err(&self) -> f64 {
        if self.n < 2 {
            return f64::INFINITY;
        }
        let n = self.n as f64;
        let var = ((self.sum_sq - self.sum * self.sum / n) / (n - 1.0)).max(0.0);
        (var / n).sqrt()
    }

    /// Half-width of the 95% interval of the mean score.
    pub fn margin(&self) -> f64 {
        if self.n < 2 { f64::INFINITY } else { t95((self.n - 1) as f64) * self.std_err() }
    }

    pub fn elo(&self) -> f64 {
        elo(self.mean())
    }

    /// 95% interval of the Elo difference.
    pub fn elo_interval(&self) -> (f64, f64) {
        let (m, w) = (self.mean(), self.margin());
        (elo(m - w), elo(m + w))
    }
}

/// Elo difference for a score fraction, clamped to stay finite.
pub(super) fn elo(p: f64) -> f64 {
    let p = p.clamp(1e-3, 1.0 - 1e-3);
    -400.0 * (1.0 / p - 1.0).log10()
}
