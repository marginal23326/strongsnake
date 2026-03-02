use super::super::common::{DeathReasonCounts, MatchResult};
use super::types::{
    ArenaDeathStats, ArenaDistributionBin, ArenaFindResult, ArenaLengthExtreme, ArenaProgress, ArenaSummary, ArenaTurnsExtreme,
};

const TURN_BIN_LABELS: [&str; 11] = [
    "0-100", "101-200", "201-300", "301-400", "401-500", "501-600", "601-700", "701-800", "801-900", "901-1000", "1000+",
];
const LENGTH_BIN_LABELS: [&str; 15] = [
    "0-5", "6-10", "11-15", "16-20", "21-25", "26-30", "31-35", "36-40", "41-45", "46-50", "51-55", "56-60", "61-65", "66-70", "70+",
];

#[derive(Debug, Default)]
pub(crate) struct ArenaAccumulator {
    wins_local: usize,
    wins_opponent: usize,
    draws: usize,
    total_turns: u64,
    total_local_length: u64,
    total_opponent_length: u64,
    death_stats: ArenaDeathStats,
    turn_bins: [usize; TURN_BIN_LABELS.len()],
    local_length_bins: [usize; LENGTH_BIN_LABELS.len()],
    opponent_length_bins: [usize; LENGTH_BIN_LABELS.len()],
    shortest_turn_game: Option<ArenaTurnsExtreme>,
    longest_turn_game: Option<ArenaTurnsExtreme>,
    shortest_local_length_game: Option<ArenaLengthExtreme>,
    longest_local_length_game: Option<ArenaLengthExtreme>,
}

#[derive(Debug)]
pub(crate) struct ArenaSummaryInput {
    pub total_games: usize,
    pub duration_ms: u128,
    pub resumed_from_snapshot: bool,
    pub snapshot_file: Option<String>,
    pub invalid_find_modes: Vec<String>,
    pub find_results: Vec<ArenaFindResult>,
    pub results: Vec<MatchResult>,
}

impl ArenaAccumulator {
    pub(crate) fn record_result(&mut self, result: &MatchResult) {
        match result.winner.as_str() {
            "local" => self.wins_local += 1,
            "opponent" => self.wins_opponent += 1,
            _ => self.draws += 1,
        }
        self.total_turns += result.turns as u64;
        self.total_local_length += result.local_length as u64;
        self.total_opponent_length += result.opp_length as u64;

        add_death_counts(&mut self.death_stats.local, &result.death.local);
        add_death_counts(&mut self.death_stats.opponent, &result.death.opponent);

        self.turn_bins[turn_bin_index(result.turns)] += 1;
        self.local_length_bins[length_bin_index(result.local_length)] += 1;
        self.opponent_length_bins[length_bin_index(result.opp_length)] += 1;

        if self.shortest_turn_game.as_ref().is_none_or(|current| result.turns < current.turns) {
            self.shortest_turn_game = Some(ArenaTurnsExtreme {
                seed: result.seed,
                turns: result.turns,
            });
        }
        if self.longest_turn_game.as_ref().is_none_or(|current| result.turns > current.turns) {
            self.longest_turn_game = Some(ArenaTurnsExtreme {
                seed: result.seed,
                turns: result.turns,
            });
        }
        if self
            .shortest_local_length_game
            .as_ref()
            .is_none_or(|current| result.local_length < current.length)
        {
            self.shortest_local_length_game = Some(ArenaLengthExtreme {
                seed: result.seed,
                length: result.local_length,
            });
        }
        if self
            .longest_local_length_game
            .as_ref()
            .is_none_or(|current| result.local_length > current.length)
        {
            self.longest_local_length_game = Some(ArenaLengthExtreme {
                seed: result.seed,
                length: result.local_length,
            });
        }
    }

    pub(crate) fn progress_snapshot(
        &self,
        completed_games: usize,
        total_games: usize,
        elapsed_ms: u128,
        last: &MatchResult,
    ) -> ArenaProgress {
        let denom = completed_games.max(1) as f64;
        ArenaProgress {
            completed_games,
            total_games,
            wins_local: self.wins_local,
            wins_opponent: self.wins_opponent,
            draws: self.draws,
            local_win_rate: percent(self.wins_local, completed_games),
            opponent_win_rate: percent(self.wins_opponent, completed_games),
            draw_rate: percent(self.draws, completed_games),
            avg_turns: self.total_turns as f64 / denom,
            avg_local_length: self.total_local_length as f64 / denom,
            avg_opponent_length: self.total_opponent_length as f64 / denom,
            elapsed_ms,
            last_seed: last.seed,
            last_turns: last.turns,
            last_winner: last.winner.clone(),
        }
    }

    pub(crate) fn into_summary(self, input: ArenaSummaryInput) -> ArenaSummary {
        let ArenaSummaryInput {
            total_games,
            duration_ms,
            resumed_from_snapshot,
            snapshot_file,
            invalid_find_modes,
            find_results,
            results,
        } = input;

        let denom = total_games.max(1) as f64;
        ArenaSummary {
            wins_local: self.wins_local,
            wins_opponent: self.wins_opponent,
            draws: self.draws,
            total_games,
            local_win_rate: percent(self.wins_local, total_games),
            opponent_win_rate: percent(self.wins_opponent, total_games),
            draw_rate: percent(self.draws, total_games),
            avg_turns: self.total_turns as f64 / denom,
            avg_local_length: self.total_local_length as f64 / denom,
            avg_opponent_length: self.total_opponent_length as f64 / denom,
            duration_ms,
            death_stats: self.death_stats,
            turn_distribution: build_distribution(&TURN_BIN_LABELS, &self.turn_bins, total_games),
            local_length_distribution: build_distribution(&LENGTH_BIN_LABELS, &self.local_length_bins, total_games),
            opponent_length_distribution: build_distribution(&LENGTH_BIN_LABELS, &self.opponent_length_bins, total_games),
            shortest_turn_game: self.shortest_turn_game,
            longest_turn_game: self.longest_turn_game,
            shortest_local_length_game: self.shortest_local_length_game,
            longest_local_length_game: self.longest_local_length_game,
            resumed_from_snapshot,
            snapshot_file,
            invalid_find_modes,
            find_results,
            results,
        }
    }
}

pub(crate) fn format_death_row(v: &DeathReasonCounts) -> String {
    format!("starvation={} wall={} body={} head={}", v.starvation, v.wall, v.body, v.head)
}

fn percent(count: usize, total: usize) -> f64 {
    if total == 0 { 0.0 } else { (count as f64 / total as f64) * 100.0 }
}

fn add_death_counts(target: &mut DeathReasonCounts, source: &DeathReasonCounts) {
    target.starvation += source.starvation;
    target.wall += source.wall;
    target.body += source.body;
    target.head += source.head;
}

fn build_distribution(labels: &[&str], counts: &[usize], total_games: usize) -> Vec<ArenaDistributionBin> {
    labels
        .iter()
        .zip(counts.iter())
        .map(|(label, count)| ArenaDistributionBin {
            label: (*label).to_owned(),
            count: *count,
            percent: percent(*count, total_games),
        })
        .collect()
}

fn turn_bin_index(turns: u32) -> usize {
    match turns {
        0..=100 => 0,
        101..=200 => 1,
        201..=300 => 2,
        301..=400 => 3,
        401..=500 => 4,
        501..=600 => 5,
        601..=700 => 6,
        701..=800 => 7,
        801..=900 => 8,
        901..=1000 => 9,
        _ => 10,
    }
}

fn length_bin_index(length: usize) -> usize {
    match length {
        0..=5 => 0,
        6..=10 => 1,
        11..=15 => 2,
        16..=20 => 3,
        21..=25 => 4,
        26..=30 => 5,
        31..=35 => 6,
        36..=40 => 7,
        41..=45 => 8,
        46..=50 => 9,
        51..=55 => 10,
        56..=60 => 11,
        61..=65 => 12,
        66..=70 => 13,
        _ => 14,
    }
}
