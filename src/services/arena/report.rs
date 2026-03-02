use std::fmt::Write as _;

use super::stats::format_death_row;
use super::types::{ArenaProgress, ArenaSummary};

pub fn format_arena_progress_line(progress: &ArenaProgress) -> String {
    format!(
        "[{}/{}] local={} opponent={} draws={} | win {:.2}% | avg turns {:.1}",
        progress.completed_games,
        progress.total_games,
        progress.wins_local,
        progress.wins_opponent,
        progress.draws,
        progress.local_win_rate,
        progress.avg_turns
    )
}

pub fn format_arena_summary_report(summary: &ArenaSummary) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "=== ARENA SUMMARY ===");
    let _ = writeln!(
        out,
        "Results: local={} opponent={} draws={} total={}",
        summary.wins_local, summary.wins_opponent, summary.draws, summary.total_games
    );
    let _ = writeln!(
        out,
        "Rates: local={:.2}% opponent={:.2}% draws={:.2}%",
        summary.local_win_rate, summary.opponent_win_rate, summary.draw_rate
    );
    let _ = writeln!(
        out,
        "Averages: turns={:.2} local_len={:.2} opponent_len={:.2}",
        summary.avg_turns, summary.avg_local_length, summary.avg_opponent_length
    );
    let _ = writeln!(out, "Duration: {} ms", summary.duration_ms);
    if summary.resumed_from_snapshot {
        let _ = writeln!(out, "Resumed: yes ({})", summary.snapshot_file.as_deref().unwrap_or("snapshot"));
    } else {
        let _ = writeln!(out, "Resumed: no");
    }
    if !summary.invalid_find_modes.is_empty() {
        let _ = writeln!(out, "Ignored invalid find modes: {}", summary.invalid_find_modes.join(", "));
    }

    if let Some(extreme) = &summary.shortest_turn_game {
        let _ = writeln!(out, "Shortest game: {} turns (seed {})", extreme.turns, extreme.seed);
    }
    if let Some(extreme) = &summary.longest_turn_game {
        let _ = writeln!(out, "Longest game: {} turns (seed {})", extreme.turns, extreme.seed);
    }
    if let Some(extreme) = &summary.shortest_local_length_game {
        let _ = writeln!(out, "Shortest local length: {} (seed {})", extreme.length, extreme.seed);
    }
    if let Some(extreme) = &summary.longest_local_length_game {
        let _ = writeln!(out, "Longest local length: {} (seed {})", extreme.length, extreme.seed);
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "Death Analysis:");
    let _ = writeln!(out, "  local    {}", format_death_row(&summary.death_stats.local));
    let _ = writeln!(out, "  opponent {}", format_death_row(&summary.death_stats.opponent));

    let _ = writeln!(out);
    let _ = writeln!(out, "Turn Distribution:");
    for bin in &summary.turn_distribution {
        let _ = writeln!(out, "  {:>8}: {:>4} ({:>6.2}%)", bin.label, bin.count, bin.percent);
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "Length Distribution (local | opponent):");
    for (local, opponent) in summary
        .local_length_distribution
        .iter()
        .zip(summary.opponent_length_distribution.iter())
    {
        let _ = writeln!(
            out,
            "  {:>8}: {:>4} ({:>6.2}%) | {:>4} ({:>6.2}%)",
            local.label, local.count, local.percent, opponent.count, opponent.percent
        );
    }

    if !summary.find_results.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Find Results:");
        for found in &summary.find_results {
            let _ = writeln!(out, "  {}: {} (winner: {})", found.mode_title, found.metric_label, found.winner);
            let _ = writeln!(out, "    Reproduce: {}", found.reproduce_hint);
            if let Some(path) = &found.snapshot_file {
                let _ = writeln!(out, "    Snapshot: {}", path);
            }
            if let Some(hint) = &found.resume_hint {
                let _ = writeln!(out, "    Resume: {}", hint);
            }
        }
    }

    out.trim_end().to_owned()
}
