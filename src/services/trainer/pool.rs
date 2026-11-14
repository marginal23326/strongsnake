use std::{
    io::Write,
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Context, Result, bail};

use super::pairs::{PairJob, PairResult, RESULT_PREFIX};

/// Runs each job in a worker process, `workers` at a time; results come back in job order.
pub(super) fn run_jobs(jobs: &[PairJob], workers: usize) -> Result<Vec<Vec<PairResult>>> {
    let exe = std::env::current_exe().context("finding the snake-app executable")?;
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Vec<PairResult>>>> = Mutex::new(vec![None; jobs.len()]);
    let failure: Mutex<Option<anyhow::Error>> = Mutex::new(None);

    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, jobs.len().max(1)) {
            scope.spawn(|| {
                loop {
                    if failure.lock().unwrap().is_some() {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(job) = jobs.get(i) else { return };
                    match run_one(&exe, job) {
                        Ok(r) => results.lock().unwrap()[i] = Some(r),
                        Err(e) => {
                            failure.lock().unwrap().get_or_insert(e);
                            return;
                        }
                    }
                }
            });
        }
    });

    if let Some(err) = failure.into_inner().unwrap() {
        return Err(err);
    }
    Ok(results.into_inner().unwrap().into_iter().map(|r| r.expect("every job ran")).collect())
}

fn run_one(exe: &std::path::Path, job: &PairJob) -> Result<Vec<PairResult>> {
    let mut child = Command::new(exe)
        .arg("trainer-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting a worker ({})", exe.display()))?;
    // The job is small enough to write in one go; dropping stdin ends the worker's read.
    child
        .stdin
        .take()
        .context("worker stdin")?
        .write_all(&serde_json::to_vec(job)?)
        .context("sending the job to a worker")?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("a worker failed ({}):\n{}", out.status, String::from_utf8_lossy(&out.stderr));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find_map(|l| l.strip_prefix(RESULT_PREFIX))
        .context("a worker printed no result")?;
    let results: Vec<PairResult> = serde_json::from_str(line).context("parsing a worker result")?;
    if results.len() != job.pairs.len() {
        bail!("a worker returned {} results for {} pairs", results.len(), job.pairs.len());
    }
    Ok(results)
}
