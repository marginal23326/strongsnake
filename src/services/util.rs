use std::path::{Path, PathBuf};

pub fn parse_depths(raw: &str) -> Vec<usize> {
    fn expand_range(a: usize, b: usize, step: usize) -> Vec<usize> {
        let from = a.min(b).max(1);
        let to = a.max(b).max(1);
        let step = step.max(1);
        let mut out = Vec::new();
        let mut v = from;
        while v <= to {
            out.push(v);
            v += step;
        }
        out
    }

    let mut out = Vec::new();
    for token in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some((lhs, rhs)) = token.split_once('-') {
            let (rhs_num, step) = if let Some((r, s)) = rhs.split_once(':') {
                (r, s.parse::<usize>().unwrap_or(1))
            } else if let Some((r, s)) = rhs.split_once('/') {
                (r, s.parse::<usize>().unwrap_or(1))
            } else {
                (rhs, 1)
            };
            if let (Ok(a), Ok(b)) = (lhs.parse::<usize>(), rhs_num.parse::<usize>()) {
                out.extend(expand_range(a, b, step));
            }
        } else if let Ok(v) = token.parse::<usize>() {
            out.push(v.max(1));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub fn default_scenario_dir(rust_root: &Path) -> PathBuf {
    rust_root.join("data").join("scenarios")
}
