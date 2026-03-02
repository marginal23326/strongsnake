mod eval;
mod genes;
mod matchups;
mod types;

use std::path::Path;

use anyhow::{Context, Result};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use serde_json::{Value, json};
use snake_ai::AiConfig;

use self::{
    eval::evaluate_candidate_with_matchups,
    genes::{apply_genes, crossover, mutate, random_chromosome, read_genes},
    matchups::build_trainer_matchups,
    types::TrainerCheckpoint,
};

pub use self::types::{TrainerOptions, TrainerSummary};

fn tournament_select(pop: &[Vec<f64>], fits: &[f64], k: usize, rng: &mut StdRng) -> Vec<f64> {
    let mut best_idx = rng.random_range(0..pop.len());
    for _ in 1..k.max(1) {
        let idx = rng.random_range(0..pop.len());
        if fits[idx] > fits[best_idx] {
            best_idx = idx;
        }
    }
    pop[best_idx].clone()
}

fn load_resume_checkpoint(options: &TrainerOptions) -> Result<Option<TrainerCheckpoint>> {
    let Some(resume_path) = &options.resume else {
        return Ok(None);
    };
    if !resume_path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(resume_path).with_context(|| format!("failed reading resume file {}", resume_path.display()))?;
    let checkpoint =
        serde_json::from_str::<TrainerCheckpoint>(&raw).with_context(|| format!("failed parsing resume file {}", resume_path.display()))?;
    Ok(Some(checkpoint))
}

fn save_checkpoint(path: &Path, checkpoint: &TrainerCheckpoint) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(checkpoint)?)
        .with_context(|| format!("failed writing temporary checkpoint {}", tmp.display()))?;
    if path.exists() {
        std::fs::remove_file(path).with_context(|| format!("failed replacing existing checkpoint {}", path.display()))?;
    }
    std::fs::rename(&tmp, path).with_context(|| format!("failed finalizing checkpoint {}", path.display()))?;
    Ok(())
}

fn trainer_settings_value(options: &TrainerOptions) -> Result<Value> {
    let mut value = serde_json::to_value(options)?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("httpApiMode".to_owned(), Value::String(format!("{:?}", options.http_api_mode)));
    }
    Ok(value)
}

fn save_training_result(path: &Path, summary: &TrainerSummary, tuned: &AiConfig, options: &TrainerOptions) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let output = json!({
        "bestFitness": summary.best_fitness,
        "bestGeneration": summary.best_generation,
        "genes": summary.genes,
        "bestConfig": tuned,
        "settings": trainer_settings_value(options)?,
    });
    std::fs::write(path, serde_json::to_string_pretty(&output)?)?;
    Ok(())
}

pub async fn run_trainer(base_cfg: AiConfig, mut options: TrainerOptions) -> Result<TrainerSummary> {
    options.normalize();

    let mut rng = StdRng::seed_from_u64(options.seed);
    let mut start_generation = 0usize;
    let mut population = Vec::new();
    let mut best_score = f64::NEG_INFINITY;
    let mut best_genes = read_genes(&base_cfg);
    let mut best_gen = 1usize;

    if let Some(checkpoint) = load_resume_checkpoint(&options)? {
        start_generation = checkpoint.generation.min(options.gens);
        population = checkpoint.pop;
        best_genes = checkpoint.best_genes;
        best_score = checkpoint.best_score;
        best_gen = checkpoint.best_generation.max(1);
        if let Some(resume_path) = &options.resume {
            println!("Resumed trainer from {} at generation {}", resume_path.display(), start_generation);
        }
    }

    if population.is_empty() {
        population.push(read_genes(&base_cfg));
    }
    while population.len() < options.pop {
        population.push(random_chromosome(&mut rng));
    }
    if population.len() > options.pop {
        population.truncate(options.pop);
    }

    let matchups = build_trainer_matchups(&base_cfg, &options);
    println!(
        "Trainer matchups: {}",
        matchups
            .iter()
            .map(|m| format!("{}x{}", m.games, m.label))
            .collect::<Vec<_>>()
            .join(", ")
    );

    for generation_idx in start_generation..options.gens {
        let mut scores = Vec::with_capacity(population.len());
        for (i, chromo) in population.iter().enumerate() {
            let s = evaluate_candidate_with_matchups(
                &base_cfg,
                chromo,
                &options,
                &matchups,
                (options.seed as u32).wrapping_add((generation_idx * 10_000 + i) as u32),
            )
            .await;
            scores.push(s);
        }

        let avg = scores.iter().sum::<f64>() / scores.len() as f64;
        let (best_idx, gen_best) = scores
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, s)| (idx, *s))
            .unwrap_or((0, 0.0));

        if gen_best > best_score {
            best_score = gen_best;
            best_genes = population[best_idx].clone();
            best_gen = generation_idx + 1;
        }

        println!(
            "Gen {:>3}/{} best {:>10.3} avg {:>10.3} best-ever {:>10.3}",
            generation_idx + 1,
            options.gens,
            gen_best,
            avg,
            best_score
        );

        let mut ranked: Vec<(Vec<f64>, f64)> = population.iter().cloned().zip(scores.iter().copied()).collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut next_pop: Vec<Vec<f64>> = ranked.iter().take(options.elite).map(|x| x.0.clone()).collect();
        while next_pop.len() < options.pop {
            let p1 = tournament_select(&population, &scores, options.tourney, &mut rng);
            let p2 = tournament_select(&population, &scores, options.tourney, &mut rng);
            let mut child = crossover(&p1, &p2, &mut rng);
            mutate(&mut child, &mut rng, options.mut_rate, options.mut_strength);
            next_pop.push(child);
        }
        population = next_pop;

        if let Some(checkpoint_path) = &options.checkpoint {
            let checkpoint = TrainerCheckpoint {
                generation: generation_idx + 1,
                pop: population.clone(),
                best_genes: best_genes.clone(),
                best_score,
                best_generation: best_gen,
            };
            save_checkpoint(checkpoint_path, &checkpoint)?;
        }
    }

    let summary = TrainerSummary {
        best_fitness: best_score,
        best_generation: best_gen,
        genes: best_genes.clone(),
    };

    if let Some(path) = &options.save {
        let tuned = apply_genes(base_cfg, &best_genes);
        save_training_result(path, &summary, &tuned, &options)?;
    }

    Ok(summary)
}
