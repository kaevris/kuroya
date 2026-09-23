use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
};

use crate::history::NavigationLocation;
use fuzzy_matcher::skim::SkimMatcherV2;

use super::{
    MAX_QUICK_OPEN_QUERY_MEMORY, QuickOpenFileNameKey, QuickOpenPathKey, QuickOpenQueryMemoryEntry,
    QuickOpenResult, labels::quick_open_relative_label, lexical_normalize_path,
    normalize_quick_open_workspace_path_with_normalized_root, query::QuickOpenMatchQuery,
    quick_open_attach_navigation_line_columns, quick_open_file_name_key,
    quick_open_insert_unseen_path_key, quick_open_normalized_path_key, quick_open_path_key,
};
#[cfg(test)]
use super::{query::normalize_quick_open_memory_query, quick_open_paths_match};

pub(super) const QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT: usize = 1024;
pub(super) const QUICK_OPEN_OPEN_FILE_SCAN_LIMIT: usize = QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT * 4;
const QUICK_OPEN_NAVIGATION_BONUS: i64 = 56;
const QUICK_OPEN_RECENT_BONUS: i64 = 30;
pub(super) const QUICK_OPEN_OPEN_FILE_BONUS: i64 = 18;
const QUICK_OPEN_QUERY_MEMORY_BONUS: i64 = 96;
const QUICK_OPEN_QUERY_MEMORY_PREFIX_BONUS: i64 = 48;
const QUICK_OPEN_QUERY_MEMORY_PREFIX_MIN_CHARS: usize = 3;
const QUICK_OPEN_QUERY_MEMORY_USE_BONUS: i64 = 8;
const QUICK_OPEN_QUERY_MEMORY_MIN_EXACT_TOTAL: i64 =
    QUICK_OPEN_QUERY_MEMORY_BONUS + QUICK_OPEN_QUERY_MEMORY_USE_BONUS;
const QUICK_OPEN_FILE_NAME_BONUS: i64 = 40;
const QUICK_OPEN_FILE_NAME_PREFIX_BONUS: i64 = 25;
const QUICK_OPEN_FILE_NAME_SEGMENT_BONUS: i64 = 16;
const QUICK_OPEN_FILE_NAME_EXACT_BONUS: i64 = 20;
const QUICK_OPEN_EMPTY_QUERY_INDEX_SCAN_LIMIT: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuickOpenMatch {
    pub(crate) rank_score: i64,
    pub(crate) fuzzy_score: i64,
}
#[cfg(test)]
pub(crate) fn quick_open_rank_score_with_open_files(
    fuzzy_score: i64,
    recent: &VecDeque<PathBuf>,
    open_files: &[&Path],
    path: &Path,
) -> i64 {
    fuzzy_score
        + quick_open_recent_bonus(recent, path)
        + quick_open_open_file_bonus(open_files, path)
}

#[cfg(test)]
pub(crate) fn quick_open_rank_score(
    fuzzy_score: i64,
    recent: &VecDeque<PathBuf>,
    open_files: &[&Path],
    query_memory: &VecDeque<QuickOpenQueryMemoryEntry>,
    query: &QuickOpenMatchQuery,
    path: &Path,
) -> i64 {
    quick_open_rank_score_with_open_files(fuzzy_score, recent, open_files, path)
        + quick_open_query_memory_bonus(query_memory, query, path)
}

#[cfg(test)]
pub(crate) fn quick_open_rank_score_with_navigation(
    fuzzy_score: i64,
    recent: &VecDeque<PathBuf>,
    open_files: &[&Path],
    query_memory: &VecDeque<QuickOpenQueryMemoryEntry>,
    navigation_locations: &[NavigationLocation],
    query: &QuickOpenMatchQuery,
    path: &Path,
) -> i64 {
    quick_open_rank_score(fuzzy_score, recent, open_files, query_memory, query, path)
        + quick_open_navigation_bonus(navigation_locations, path)
}

#[derive(Debug, Default)]
pub(super) struct QuickOpenRankingBonusContext {
    bonuses_by_exact_path: HashMap<PathBuf, i64>,
    bonuses_by_path: HashMap<QuickOpenPathKey, i64>,
    exact_paths_by_key: HashMap<QuickOpenPathKey, Vec<PathBuf>>,
    bonus_file_names: HashSet<QuickOpenFileNameKey>,
}

#[derive(Debug)]
pub(super) struct QuickOpenCandidateRankData<'a> {
    path: &'a Path,
    path_key: Option<QuickOpenPathKey>,
    file_name_key: Option<Option<QuickOpenFileNameKey>>,
}

impl<'a> QuickOpenCandidateRankData<'a> {
    pub(super) fn new(path: &'a Path) -> Self {
        Self {
            path,
            path_key: None,
            file_name_key: None,
        }
    }

    pub(super) fn path_key(&mut self) -> &QuickOpenPathKey {
        if self.path_key.is_none() {
            self.path_key = Some(quick_open_path_key(self.path));
        }
        self.path_key.as_ref().expect("path key is initialized")
    }

    fn file_name_key(&mut self) -> Option<&QuickOpenFileNameKey> {
        if self.file_name_key.is_none() {
            self.file_name_key = Some(quick_open_file_name_key(self.path));
        }
        self.file_name_key.as_ref().and_then(Option::as_ref)
    }
}

impl QuickOpenRankingBonusContext {
    pub(super) fn new<'a>(
        recent: &VecDeque<PathBuf>,
        open_files: impl IntoIterator<Item = &'a Path>,
        query_memory: &VecDeque<QuickOpenQueryMemoryEntry>,
        navigation_locations: &[NavigationLocation],
        query: &QuickOpenMatchQuery,
    ) -> Self {
        let open_files = open_files.into_iter();
        let capacity = recent
            .len()
            .min(QUICK_OPEN_RECENT_BONUS as usize)
            .saturating_add(
                open_files
                    .size_hint()
                    .0
                    .min(QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT),
            )
            .saturating_add(query_memory.len().min(MAX_QUICK_OPEN_QUERY_MEMORY))
            .saturating_add(
                navigation_locations
                    .len()
                    .min(QUICK_OPEN_NAVIGATION_BONUS as usize),
            );
        let mut context = Self::with_capacity(capacity);
        context.add_recent_bonuses(recent);
        context.add_open_file_bonuses(open_files);
        context.add_query_memory_bonuses(query_memory, query);
        context.add_navigation_bonuses(navigation_locations);
        context
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            bonuses_by_exact_path: HashMap::with_capacity(capacity),
            bonuses_by_path: HashMap::with_capacity(capacity),
            exact_paths_by_key: HashMap::with_capacity(capacity),
            bonus_file_names: HashSet::with_capacity(capacity),
        }
    }

    pub(super) fn rank_score(&self, fuzzy_score: i64, path: &Path) -> i64 {
        let mut candidate = QuickOpenCandidateRankData::new(path);
        self.rank_score_for_candidate(fuzzy_score, &mut candidate)
    }

    pub(super) fn rank_score_for_candidate(
        &self,
        fuzzy_score: i64,
        candidate: &mut QuickOpenCandidateRankData<'_>,
    ) -> i64 {
        fuzzy_score + self.rank_bonus_for_candidate(candidate).unwrap_or_default()
    }

    fn rank_bonus_for_candidate(
        &self,
        candidate: &mut QuickOpenCandidateRankData<'_>,
    ) -> Option<i64> {
        self.bonuses_by_exact_path
            .get(candidate.path)
            .copied()
            .or_else(|| self.keyed_rank_bonus(candidate))
    }

    fn is_empty(&self) -> bool {
        self.bonuses_by_exact_path.is_empty() && self.bonuses_by_path.is_empty()
    }

    pub(super) fn bonus_path_keys(&self) -> HashSet<QuickOpenPathKey> {
        let mut keys = HashSet::with_capacity(self.bonuses_by_path.len());
        keys.extend(self.bonuses_by_path.keys().cloned());
        keys
    }

    fn add_bonus_for_key(&mut self, path: &Path, path_key: QuickOpenPathKey, bonus: i64) {
        if bonus > 0 {
            self.remember_exact_path_for_key(&path_key, path);
            self.add_key_bonus(path_key, bonus);
        }
    }

    fn add_exact_bonus(&mut self, path: &Path, bonus: i64) {
        if bonus > 0 {
            *self
                .bonuses_by_exact_path
                .entry(path.to_path_buf())
                .or_default() += bonus;
        }
    }

    fn add_key_bonus(&mut self, path: QuickOpenPathKey, bonus: i64) {
        if bonus > 0 {
            if let Some(exact_paths) = self.exact_paths_by_key.get(&path) {
                for exact_path in exact_paths {
                    *self
                        .bonuses_by_exact_path
                        .entry(exact_path.clone())
                        .or_default() += bonus;
                }
            }
            *self.bonuses_by_path.entry(path).or_default() += bonus;
        }
    }

    fn remember_exact_path_for_key(&mut self, key: &QuickOpenPathKey, path: &Path) {
        let exact_paths = self.exact_paths_by_key.entry(key.clone()).or_default();
        if exact_paths.iter().any(|existing| existing == path) {
            return;
        }

        exact_paths.push(path.to_path_buf());
        if let Some(existing_bonus) = self.bonuses_by_path.get(key).copied() {
            self.add_exact_bonus(path, existing_bonus);
        }
        self.add_bonus_file_name(path);
    }

    fn add_bonus_file_name(&mut self, path: &Path) {
        if let Some(file_name) = quick_open_file_name_key(path) {
            self.bonus_file_names.insert(file_name);
        }
    }

    fn keyed_rank_bonus(&self, candidate: &mut QuickOpenCandidateRankData<'_>) -> Option<i64> {
        if self.bonus_file_names.is_empty() {
            return None;
        }

        let file_name_matches = candidate
            .file_name_key()
            .is_some_and(|file_name| self.bonus_file_names.contains(file_name));
        if !file_name_matches {
            return None;
        }
        self.bonuses_by_path.get(candidate.path_key()).copied()
    }

    fn add_recent_bonuses(&mut self, recent: &VecDeque<PathBuf>) {
        let mut seen = HashSet::with_capacity(recent.len().min(QUICK_OPEN_RECENT_BONUS as usize));
        for (index, path) in recent.iter().enumerate() {
            let bonus = (QUICK_OPEN_RECENT_BONUS - index as i64).max(0);
            if bonus == 0 {
                break;
            }

            let path_key = quick_open_path_key(path);
            if quick_open_insert_unseen_path_key(&mut seen, &path_key) {
                self.add_bonus_for_key(path, path_key, bonus);
            }
        }
    }

    fn add_open_file_bonuses<'a>(&mut self, open_files: impl IntoIterator<Item = &'a Path>) {
        let open_files = open_files.into_iter();
        let mut seen = HashSet::with_capacity(
            open_files
                .size_hint()
                .0
                .min(QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT),
        );
        for path in open_files.take(QUICK_OPEN_OPEN_FILE_SCAN_LIMIT) {
            let path_key = quick_open_path_key(path);
            if quick_open_insert_unseen_path_key(&mut seen, &path_key) {
                self.add_bonus_for_key(path, path_key, QUICK_OPEN_OPEN_FILE_BONUS);
                if seen.len() >= QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT {
                    break;
                }
            }
        }
    }

    fn add_query_memory_bonuses(
        &mut self,
        memory: &VecDeque<QuickOpenQueryMemoryEntry>,
        query: &QuickOpenMatchQuery,
    ) {
        let Some(query) = query.normalized_memory_query.as_deref() else {
            return;
        };
        let mut memory_bonuses = HashMap::<QuickOpenPathKey, i64>::with_capacity(
            memory.len().min(MAX_QUICK_OPEN_QUERY_MEMORY),
        );
        for (index, entry) in memory.iter().enumerate() {
            let query_affinity = quick_open_memory_query_affinity(&entry.query, query);
            if query_affinity == 0 {
                continue;
            }
            let uses = i64::from(entry.uses.clamp(1, 8));
            let recency = (MAX_QUICK_OPEN_QUERY_MEMORY.saturating_sub(index) as i64).min(24);
            let score = query_affinity + (uses * QUICK_OPEN_QUERY_MEMORY_USE_BONUS) + recency;
            let bonus = if query_affinity == QUICK_OPEN_QUERY_MEMORY_PREFIX_BONUS {
                score.min(QUICK_OPEN_QUERY_MEMORY_MIN_EXACT_TOTAL - 1)
            } else {
                score
            };
            let entry_path = entry.path.as_path();
            let path_key = quick_open_path_key(entry_path);
            self.remember_exact_path_for_key(&path_key, entry_path);
            memory_bonuses
                .entry(path_key)
                .and_modify(|existing| *existing = (*existing).max(bonus))
                .or_insert(bonus);
        }
        for (path_key, bonus) in memory_bonuses {
            self.add_key_bonus(path_key, bonus);
        }
    }

    fn add_navigation_bonuses(&mut self, navigation_locations: &[NavigationLocation]) {
        let mut seen = HashSet::with_capacity(
            navigation_locations
                .len()
                .min(QUICK_OPEN_NAVIGATION_BONUS as usize),
        );
        for (index, location) in navigation_locations.iter().rev().enumerate() {
            let bonus = (QUICK_OPEN_NAVIGATION_BONUS - index as i64).max(0);
            if bonus == 0 {
                break;
            }

            let path_key = quick_open_path_key(&location.path);
            if quick_open_insert_unseen_path_key(&mut seen, &path_key) {
                self.add_bonus_for_key(&location.path, path_key, bonus);
            }
        }
    }
}

#[cfg(test)]
fn quick_open_recent_bonus(recent: &VecDeque<PathBuf>, path: &Path) -> i64 {
    recent
        .iter()
        .position(|entry| quick_open_paths_match(entry, path))
        .map(|index| (QUICK_OPEN_RECENT_BONUS - index as i64).max(0))
        .unwrap_or_default()
}

#[cfg(test)]
fn quick_open_open_file_bonus(open_files: &[&Path], path: &Path) -> i64 {
    if open_files
        .iter()
        .any(|open_file| quick_open_paths_match(open_file, path))
    {
        QUICK_OPEN_OPEN_FILE_BONUS
    } else {
        0
    }
}

#[cfg(test)]
fn quick_open_query_memory_bonus(
    memory: &VecDeque<QuickOpenQueryMemoryEntry>,
    query: &QuickOpenMatchQuery,
    path: &Path,
) -> i64 {
    let Some(query) = normalize_quick_open_memory_query(&query.raw) else {
        return 0;
    };
    memory
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            if !quick_open_paths_match(&entry.path, path) {
                return None;
            }
            let query_affinity = quick_open_memory_query_affinity(&entry.query, &query);
            if query_affinity == 0 {
                return None;
            }
            let uses = i64::from(entry.uses.clamp(1, 8));
            let recency = (MAX_QUICK_OPEN_QUERY_MEMORY.saturating_sub(index) as i64).min(24);
            let score = query_affinity + (uses * QUICK_OPEN_QUERY_MEMORY_USE_BONUS) + recency;
            Some(if query_affinity == QUICK_OPEN_QUERY_MEMORY_PREFIX_BONUS {
                score.min(QUICK_OPEN_QUERY_MEMORY_MIN_EXACT_TOTAL - 1)
            } else {
                score
            })
        })
        .max()
        .unwrap_or_default()
}

fn quick_open_memory_query_affinity(memory_query: &str, current_query: &str) -> i64 {
    if memory_query == current_query {
        QUICK_OPEN_QUERY_MEMORY_BONUS
    } else if quick_open_memory_queries_share_prefix(memory_query, current_query) {
        QUICK_OPEN_QUERY_MEMORY_PREFIX_BONUS
    } else {
        0
    }
}

fn quick_open_memory_queries_share_prefix(memory_query: &str, current_query: &str) -> bool {
    (memory_query.starts_with(current_query) || current_query.starts_with(memory_query))
        && memory_query
            .chars()
            .zip(current_query.chars())
            .take_while(|(left, right)| left == right)
            .count()
            >= QUICK_OPEN_QUERY_MEMORY_PREFIX_MIN_CHARS
}

#[cfg(test)]
pub(crate) fn quick_open_navigation_target<'a>(
    navigation_locations: &'a [NavigationLocation],
    path: &Path,
) -> Option<&'a NavigationLocation> {
    navigation_locations
        .iter()
        .rev()
        .find(|location| quick_open_paths_match(&location.path, path))
}

#[cfg(test)]
pub(crate) fn quick_open_latest_navigation_locations_by_path(
    navigation_locations: &[NavigationLocation],
) -> Vec<NavigationLocation> {
    let mut seen = HashSet::new();
    let mut latest = Vec::new();
    for location in navigation_locations.iter().rev() {
        push_latest_quick_open_navigation_location(&mut latest, &mut seen, location);
    }

    latest.reverse();
    latest
}

pub(crate) fn quick_open_latest_navigation_locations_from_history(
    back: &VecDeque<NavigationLocation>,
    forward: &VecDeque<NavigationLocation>,
    current: Option<&NavigationLocation>,
) -> Vec<NavigationLocation> {
    let capacity = quick_open_navigation_history_capacity(back, forward, current);
    let mut seen = HashSet::with_capacity(capacity);
    let mut latest = Vec::with_capacity(capacity);
    if let Some(current) = current {
        push_latest_quick_open_navigation_location(&mut latest, &mut seen, current);
    }
    for location in forward.iter().rev() {
        push_latest_quick_open_navigation_location(&mut latest, &mut seen, location);
    }
    for location in back.iter().rev() {
        push_latest_quick_open_navigation_location(&mut latest, &mut seen, location);
    }

    latest.reverse();
    latest
}

fn quick_open_navigation_history_capacity(
    back: &VecDeque<NavigationLocation>,
    forward: &VecDeque<NavigationLocation>,
    current: Option<&NavigationLocation>,
) -> usize {
    back.len()
        .saturating_add(forward.len())
        .saturating_add(usize::from(current.is_some()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct QuickOpenNavigationRankKeys {
    keys: Vec<QuickOpenPathKey>,
}

impl QuickOpenNavigationRankKeys {
    pub(super) fn from_history(
        back: &VecDeque<NavigationLocation>,
        forward: &VecDeque<NavigationLocation>,
        current: Option<&NavigationLocation>,
    ) -> Self {
        let capacity = quick_open_navigation_history_capacity(back, forward, current);
        let mut seen = HashSet::with_capacity(capacity);
        let mut keys = Vec::with_capacity(capacity);
        if let Some(current) = current {
            Self::push_latest_path_key(&mut keys, &mut seen, &current.path);
        }
        for location in forward.iter().rev() {
            Self::push_latest_path_key(&mut keys, &mut seen, &location.path);
        }
        for location in back.iter().rev() {
            Self::push_latest_path_key(&mut keys, &mut seen, &location.path);
        }

        keys.reverse();
        Self { keys }
    }

    pub(super) fn matches_locations(&self, locations: &[NavigationLocation]) -> bool {
        self.keys.len() == locations.len()
            && self
                .keys
                .iter()
                .zip(locations)
                .all(|(key, location)| key == &quick_open_path_key(&location.path))
    }

    fn push_latest_path_key(
        keys: &mut Vec<QuickOpenPathKey>,
        seen: &mut HashSet<QuickOpenPathKey>,
        path: &Path,
    ) {
        let key = quick_open_path_key(path);
        if seen.insert(key.clone()) {
            keys.push(key);
        }
    }
}

fn push_latest_quick_open_navigation_location(
    latest: &mut Vec<NavigationLocation>,
    seen: &mut HashSet<QuickOpenPathKey>,
    location: &NavigationLocation,
) {
    if seen.insert(quick_open_path_key(&location.path)) {
        latest.push(location.clone());
    }
}

#[cfg(test)]
fn quick_open_navigation_bonus(navigation_locations: &[NavigationLocation], path: &Path) -> i64 {
    navigation_locations
        .iter()
        .rev()
        .position(|location| quick_open_paths_match(&location.path, path))
        .map(|index| (QUICK_OPEN_NAVIGATION_BONUS - index as i64).max(0))
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn quick_open_ranked_results<'a>(
    matcher: &SkimMatcherV2,
    workspace_root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
    recent: &VecDeque<PathBuf>,
    open_files: &[&Path],
    query_memory: &VecDeque<QuickOpenQueryMemoryEntry>,
    navigation_locations: &[NavigationLocation],
    query: &QuickOpenMatchQuery,
    limit: usize,
) -> Vec<QuickOpenResult> {
    quick_open_ranked_results_with_open_files(
        matcher,
        workspace_root,
        files,
        recent,
        open_files.iter().copied(),
        query_memory,
        navigation_locations,
        query,
        limit,
    )
}

pub(crate) fn quick_open_ranked_results_from_open_paths<'a>(
    matcher: &SkimMatcherV2,
    workspace_root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
    recent: &VecDeque<PathBuf>,
    open_files: &[PathBuf],
    query_memory: &VecDeque<QuickOpenQueryMemoryEntry>,
    navigation_locations: &[NavigationLocation],
    query: &QuickOpenMatchQuery,
    limit: usize,
) -> Vec<QuickOpenResult> {
    quick_open_ranked_results_with_open_files(
        matcher,
        workspace_root,
        files,
        recent,
        open_files.iter().map(PathBuf::as_path),
        query_memory,
        navigation_locations,
        query,
        limit,
    )
}

fn quick_open_ranked_results_with_open_files<'a, 'b, OpenFiles>(
    matcher: &SkimMatcherV2,
    workspace_root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
    recent: &VecDeque<PathBuf>,
    open_files: OpenFiles,
    query_memory: &VecDeque<QuickOpenQueryMemoryEntry>,
    navigation_locations: &[NavigationLocation],
    query: &QuickOpenMatchQuery,
    limit: usize,
) -> Vec<QuickOpenResult>
where
    OpenFiles: Clone + IntoIterator<Item = &'b Path>,
{
    if limit == 0 {
        return Vec::new();
    }
    if query.raw.is_empty() {
        return quick_open_empty_query_ranked_results(
            workspace_root,
            files,
            recent,
            open_files,
            query_memory,
            navigation_locations,
            query,
            limit,
        );
    }

    let bonus_context = QuickOpenRankingBonusContext::new(
        recent,
        open_files.clone(),
        query_memory,
        navigation_locations,
        query,
    );
    let mut top: BinaryHeap<Reverse<QuickOpenResult>> =
        BinaryHeap::with_capacity(limit.saturating_add(1));
    quick_open_for_each_candidate_path(workspace_root, files, open_files, |path| {
        let rel = quick_open_relative_label(workspace_root, path);
        let Some(match_score) = quick_open_match_score(matcher, &rel, query) else {
            return;
        };
        let rank_score = bonus_context.rank_score(match_score.rank_score, path);
        if top.len() >= limit
            && top.peek().is_some_and(|worst| {
                !quick_open_candidate_beats_result(
                    rank_score,
                    match_score.fuzzy_score,
                    &rel,
                    path,
                    &worst.0,
                )
            })
        {
            return;
        }

        let result = QuickOpenResult {
            rank_score,
            fuzzy_score: match_score.fuzzy_score,
            path: path.to_path_buf(),
            rel: rel.into_owned(),
            navigation_line_column: None,
        };

        if top.len() < limit {
            top.push(Reverse(result));
        } else if top.peek().is_some_and(|worst| result > worst.0) {
            top.pop();
            top.push(Reverse(result));
        }
    });

    let mut results = Vec::with_capacity(top.len());
    results.extend(top.into_iter().map(|Reverse(result)| result));
    results.sort_by(|left, right| right.cmp(left));
    quick_open_attach_navigation_line_columns(&mut results, navigation_locations);
    results
}

pub(super) fn quick_open_candidate_beats_result(
    rank_score: i64,
    fuzzy_score: i64,
    rel: &str,
    path: &Path,
    other: &QuickOpenResult,
) -> bool {
    rank_score
        .cmp(&other.rank_score)
        .then(fuzzy_score.cmp(&other.fuzzy_score))
        .then_with(|| other.rel.as_str().cmp(rel))
        .then_with(|| other.path.as_path().cmp(path))
        .then_with(|| None::<(usize, usize)>.cmp(&other.navigation_line_column))
        .is_gt()
}

pub(super) fn quick_open_empty_query_ranked_results<'a, 'b, OpenFiles>(
    workspace_root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
    recent: &VecDeque<PathBuf>,
    open_files: OpenFiles,
    query_memory: &VecDeque<QuickOpenQueryMemoryEntry>,
    navigation_locations: &[NavigationLocation],
    query: &QuickOpenMatchQuery,
    limit: usize,
) -> Vec<QuickOpenResult>
where
    OpenFiles: Clone + IntoIterator<Item = &'b Path>,
{
    if limit == 0 {
        return Vec::new();
    }

    let bonus_context = QuickOpenRankingBonusContext::new(
        recent,
        open_files.clone(),
        query_memory,
        navigation_locations,
        query,
    );
    if bonus_context.is_empty() {
        return quick_open_unboosted_empty_query_results(workspace_root, files, limit);
    }

    let mut remaining_bonus_keys = bonus_context.bonus_path_keys();
    let mut boosted = Vec::with_capacity(remaining_bonus_keys.len());
    let mut fallback = Vec::with_capacity(limit);
    let mut seen_paths = HashSet::with_capacity(limit.saturating_add(remaining_bonus_keys.len()));

    for candidate in quick_open_open_file_candidates(workspace_root, open_files.clone()) {
        quick_open_push_empty_query_candidate(
            workspace_root,
            candidate.path.as_path(),
            &bonus_context,
            &mut remaining_bonus_keys,
            &mut seen_paths,
            &mut boosted,
            &mut fallback,
            limit,
        );
    }

    let index_scan_limit = quick_open_empty_query_index_scan_limit(limit);
    let mut scanned = 0usize;
    for path in files {
        scanned = scanned.saturating_add(1);
        quick_open_push_empty_query_candidate(
            workspace_root,
            path,
            &bonus_context,
            &mut remaining_bonus_keys,
            &mut seen_paths,
            &mut boosted,
            &mut fallback,
            limit,
        );

        if fallback.len() >= limit
            && (remaining_bonus_keys.is_empty() || scanned >= index_scan_limit)
        {
            break;
        }
    }

    boosted.extend(fallback);
    boosted.sort_by(|left, right| right.cmp(left));
    boosted.truncate(limit);
    quick_open_attach_navigation_line_columns(&mut boosted, navigation_locations);
    boosted
}

pub(super) fn quick_open_empty_query_index_scan_limit(limit: usize) -> usize {
    QUICK_OPEN_EMPTY_QUERY_INDEX_SCAN_LIMIT.max(limit)
}

fn quick_open_push_empty_query_candidate(
    workspace_root: &Path,
    path: &Path,
    bonus_context: &QuickOpenRankingBonusContext,
    remaining_bonus_keys: &mut HashSet<QuickOpenPathKey>,
    seen_paths: &mut HashSet<QuickOpenPathKey>,
    boosted: &mut Vec<QuickOpenResult>,
    fallback: &mut Vec<QuickOpenResult>,
    limit: usize,
) {
    let mut rank_data = QuickOpenCandidateRankData::new(path);
    let rank_score = bonus_context.rank_score_for_candidate(0, &mut rank_data);
    if rank_score <= 0 && fallback.len() >= limit {
        return;
    }

    let path_key = rank_data.path_key();
    if !quick_open_insert_unseen_path_key(seen_paths, path_key) {
        return;
    }

    let rel = quick_open_relative_label(workspace_root, path);
    let result = QuickOpenResult {
        rank_score,
        fuzzy_score: 0,
        path: path.to_path_buf(),
        rel: rel.into_owned(),
        navigation_line_column: None,
    };
    if rank_score > 0 {
        remaining_bonus_keys.remove(path_key);
        boosted.push(result);
    } else {
        fallback.push(result);
    }
}

pub(super) fn quick_open_unboosted_empty_query_results<'a>(
    workspace_root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
    limit: usize,
) -> Vec<QuickOpenResult> {
    if limit == 0 {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(limit);
    let mut seen_paths = HashSet::with_capacity(limit);
    for path in files
        .into_iter()
        .take(quick_open_empty_query_index_scan_limit(limit))
    {
        if !seen_paths.insert(quick_open_path_key(path)) {
            continue;
        }

        let rel = quick_open_relative_label(workspace_root, path);
        results.push(QuickOpenResult {
            rank_score: 0,
            fuzzy_score: 0,
            path: path.to_path_buf(),
            rel: rel.into_owned(),
            navigation_line_column: None,
        });
        if results.len() >= limit {
            break;
        }
    }

    results.sort_by(|left, right| right.cmp(left));
    results
}

pub(super) fn quick_open_for_each_candidate_path<'a, 'b>(
    workspace_root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
    open_files: impl IntoIterator<Item = &'b Path>,
    mut visit: impl FnMut(&Path),
) {
    quick_open_for_each_candidate_path_while(workspace_root, files, open_files, |path| {
        visit(path);
        true
    });
}

fn quick_open_for_each_candidate_path_while<'a, 'b>(
    workspace_root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
    open_files: impl IntoIterator<Item = &'b Path>,
    mut visit: impl FnMut(&Path) -> bool,
) {
    let mut open_candidates = quick_open_open_file_candidates(workspace_root, open_files);
    let mut files = files.into_iter();
    if open_candidates.is_empty() {
        for path in files {
            if !visit(path) {
                return;
            }
        }
        return;
    }

    let mut pending_open_candidates = QuickOpenPendingOpenFileCandidates::new(&open_candidates);
    for path in files.by_ref() {
        pending_open_candidates.mark_indexed_path(&mut open_candidates, path);
        if !visit(path) {
            return;
        }

        if pending_open_candidates.is_empty() {
            for path in files {
                if !visit(path) {
                    return;
                }
            }
            return;
        }
    }

    for candidate in open_candidates {
        if !candidate.indexed {
            if !visit(candidate.path.as_path()) {
                return;
            }
        }
    }
}

#[derive(Debug)]
pub(super) struct QuickOpenOpenFileCandidate {
    pub(super) path: PathBuf,
    pub(super) key: QuickOpenPathKey,
    pub(super) file_name: Option<QuickOpenFileNameKey>,
    pub(super) indexed: bool,
}

#[derive(Debug)]
struct QuickOpenPendingOpenFileCandidates {
    candidate_indices_by_key: HashMap<QuickOpenPathKey, usize>,
    file_name_counts: HashMap<QuickOpenFileNameKey, usize>,
    name_less_count: usize,
}

impl QuickOpenPendingOpenFileCandidates {
    fn new(open_candidates: &[QuickOpenOpenFileCandidate]) -> Self {
        let mut candidate_indices_by_key = HashMap::with_capacity(open_candidates.len());
        let mut file_name_counts = HashMap::with_capacity(open_candidates.len());
        let mut name_less_count = 0usize;

        for (index, candidate) in open_candidates.iter().enumerate() {
            candidate_indices_by_key.insert(candidate.key.clone(), index);
            if let Some(file_name) = candidate.file_name.clone() {
                *file_name_counts.entry(file_name).or_insert(0) += 1;
            } else {
                name_less_count += 1;
            }
        }

        Self {
            candidate_indices_by_key,
            file_name_counts,
            name_less_count,
        }
    }

    fn mark_indexed_path(
        &mut self,
        open_candidates: &mut [QuickOpenOpenFileCandidate],
        path: &Path,
    ) {
        if self.is_empty() || !self.should_probe_path(path) {
            return;
        }

        let key = quick_open_path_key(path);
        let Some(candidate_index) = self.candidate_indices_by_key.remove(&key) else {
            return;
        };

        let candidate = &mut open_candidates[candidate_index];
        let file_name = candidate.file_name.take();
        candidate.indexed = true;
        self.remove_candidate_file_name(file_name);
    }

    fn should_probe_path(&self, path: &Path) -> bool {
        if self.name_less_count > 0 {
            return true;
        }

        quick_open_file_name_key(path)
            .as_ref()
            .is_some_and(|file_name| self.file_name_counts.contains_key(file_name))
    }

    fn remove_candidate_file_name(&mut self, file_name: Option<QuickOpenFileNameKey>) {
        let Some(file_name) = file_name else {
            self.name_less_count = self.name_less_count.saturating_sub(1);
            return;
        };

        if let Some(count) = self.file_name_counts.get_mut(&file_name) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.file_name_counts.remove(&file_name);
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.candidate_indices_by_key.is_empty()
    }
}

pub(super) fn quick_open_open_file_candidates<'a>(
    workspace_root: &Path,
    open_files: impl IntoIterator<Item = &'a Path>,
) -> Vec<QuickOpenOpenFileCandidate> {
    let workspace_root = lexical_normalize_path(workspace_root);
    let open_files = open_files.into_iter();
    let capacity = open_files
        .size_hint()
        .0
        .min(QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT);
    let mut seen = HashSet::with_capacity(capacity);
    let mut candidates = Vec::with_capacity(capacity);
    for path in open_files.take(QUICK_OPEN_OPEN_FILE_SCAN_LIMIT) {
        let Some(path) =
            normalize_quick_open_workspace_path_with_normalized_root(&workspace_root, path)
        else {
            continue;
        };

        let key = quick_open_normalized_path_key(&path);
        if seen.insert(key.clone()) {
            let file_name = quick_open_file_name_key(&path);
            candidates.push(QuickOpenOpenFileCandidate {
                path,
                key,
                file_name,
                indexed: false,
            });
            if candidates.len() >= QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT {
                break;
            }
        }
    }
    candidates
}

pub(crate) fn quick_open_match_score(
    matcher: &SkimMatcherV2,
    rel: &str,
    query: &QuickOpenMatchQuery,
) -> Option<QuickOpenMatch> {
    if !query.tokens.is_empty() {
        return quick_open_tokenized_match_score(matcher, rel, query);
    }

    let fuzzy_score = crate::fuzzy::fuzzy_match_with_case_fallback(matcher, rel, &query.raw)?;
    let mut rank_score = fuzzy_score;
    if !query.raw.is_empty() {
        let file_name = quick_open_file_name(rel);
        if let Some(file_name_score) =
            crate::fuzzy::fuzzy_match_with_case_fallback(matcher, file_name, &query.raw)
        {
            rank_score = rank_score.max(file_name_score + QUICK_OPEN_FILE_NAME_BONUS);
            let file_name_match =
                quick_open_lowercase_match_kind(file_name, query.lowercase.as_str());
            if file_name_match.starts_with_query {
                rank_score += QUICK_OPEN_FILE_NAME_PREFIX_BONUS;
            } else if quick_open_lowercase_word_start_match(file_name, query.lowercase.as_str()) {
                rank_score += QUICK_OPEN_FILE_NAME_SEGMENT_BONUS;
            }
            if file_name_match.exact {
                rank_score += QUICK_OPEN_FILE_NAME_EXACT_BONUS;
            }
        }
    }

    Some(QuickOpenMatch {
        rank_score,
        fuzzy_score,
    })
}

fn quick_open_tokenized_match_score(
    matcher: &SkimMatcherV2,
    rel: &str,
    query: &QuickOpenMatchQuery,
) -> Option<QuickOpenMatch> {
    let file_name = quick_open_file_name(rel);
    let mut fuzzy_score = 0;
    let mut rank_score = 0;

    for (token, token_lowercase) in query.tokens.iter().zip(query.token_lowercases.iter()) {
        let token_fuzzy_score = crate::fuzzy::fuzzy_match_with_case_fallback(matcher, rel, token)?;
        let mut token_rank_score = token_fuzzy_score;
        if let Some(file_name_score) =
            crate::fuzzy::fuzzy_match_with_case_fallback(matcher, file_name, token)
        {
            token_rank_score = token_rank_score.max(file_name_score + QUICK_OPEN_FILE_NAME_BONUS);
            let file_name_match =
                quick_open_lowercase_match_kind(file_name, token_lowercase.as_str());
            if file_name_match.starts_with_query {
                token_rank_score += QUICK_OPEN_FILE_NAME_PREFIX_BONUS;
            } else if quick_open_lowercase_word_start_match(file_name, token_lowercase.as_str()) {
                token_rank_score += QUICK_OPEN_FILE_NAME_SEGMENT_BONUS;
            }
            if file_name_match.exact {
                token_rank_score += QUICK_OPEN_FILE_NAME_EXACT_BONUS;
            }
        }

        fuzzy_score += token_fuzzy_score;
        rank_score += token_rank_score;
    }

    Some(QuickOpenMatch {
        rank_score,
        fuzzy_score,
    })
}

fn quick_open_file_name(rel: &str) -> &str {
    rel.rsplit(['/', '\\']).next().unwrap_or(rel)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct QuickOpenLowercaseMatch {
    pub(super) starts_with_query: bool,
    pub(super) exact: bool,
}

pub(super) fn quick_open_lowercase_match_kind(
    candidate: &str,
    query_lowercase: &str,
) -> QuickOpenLowercaseMatch {
    if candidate.is_ascii() && query_lowercase.is_ascii() {
        return quick_open_ascii_lowercase_match_kind(candidate, query_lowercase);
    }

    let mut candidate = candidate.chars().flat_map(char::to_lowercase);
    let mut query = query_lowercase.chars();
    loop {
        match query.next() {
            Some(expected) => match candidate.next() {
                Some(actual) if actual == expected => {}
                _ => {
                    return QuickOpenLowercaseMatch {
                        starts_with_query: false,
                        exact: false,
                    };
                }
            },
            None => {
                return QuickOpenLowercaseMatch {
                    starts_with_query: true,
                    exact: candidate.next().is_none(),
                };
            }
        }
    }
}

fn quick_open_ascii_lowercase_match_kind(
    candidate: &str,
    query_lowercase: &str,
) -> QuickOpenLowercaseMatch {
    let starts_with_query = candidate
        .get(..query_lowercase.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(query_lowercase));
    QuickOpenLowercaseMatch {
        starts_with_query,
        exact: starts_with_query && candidate.len() == query_lowercase.len(),
    }
}

pub(crate) fn quick_open_lowercase_word_start_match(
    candidate: &str,
    query_lowercase: &str,
) -> bool {
    if query_lowercase.is_empty() {
        return false;
    }

    if candidate.is_ascii() && query_lowercase.is_ascii() {
        return quick_open_ascii_word_start_match(candidate, query_lowercase);
    }

    candidate.char_indices().any(|(idx, ch)| {
        quick_open_word_start(candidate, idx, ch)
            && quick_open_lowercase_match_kind(&candidate[idx..], query_lowercase).starts_with_query
    })
}

fn quick_open_ascii_word_start_match(candidate: &str, query_lowercase: &str) -> bool {
    let bytes = candidate.as_bytes();
    let query_len = query_lowercase.len();
    bytes.iter().enumerate().any(|(idx, &byte)| {
        quick_open_ascii_word_start(bytes, idx, byte)
            && candidate
                .get(idx..idx.saturating_add(query_len))
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(query_lowercase))
    })
}

fn quick_open_ascii_word_start(candidate: &[u8], idx: usize, byte: u8) -> bool {
    if idx == 0 {
        return true;
    }

    let previous = candidate[idx - 1];
    !previous.is_ascii_alphanumeric()
        || (previous.is_ascii_lowercase() && byte.is_ascii_uppercase())
}

fn quick_open_word_start(candidate: &str, idx: usize, ch: char) -> bool {
    if idx == 0 {
        return true;
    }

    let previous = candidate[..idx].chars().next_back();
    previous.is_some_and(|previous| {
        !previous.is_alphanumeric() || (previous.is_lowercase() && ch.is_uppercase())
    })
}
