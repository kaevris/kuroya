use super::{FoldedRange, normalize_folded_ranges};
use crate::persistence::{BufferFoldState, PersistedFoldRange};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub(crate) fn session_fold_states(
    folded_ranges: &HashMap<PathBuf, Vec<FoldedRange>>,
) -> Vec<BufferFoldState> {
    let mut entries = folded_ranges
        .iter()
        .filter(|(path, _)| has_session_path_identity(path))
        .collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(path, _)| *path);

    entries
        .into_iter()
        .filter_map(|(path, ranges)| {
            let ranges = persisted_fold_ranges(ranges);
            (!ranges.is_empty()).then(|| BufferFoldState {
                path: path.clone(),
                ranges,
            })
        })
        .collect()
}

fn persisted_fold_ranges(ranges: &[FoldedRange]) -> Vec<PersistedFoldRange> {
    let mut ranges = ranges.to_vec();
    normalize_session_fold_ranges(&mut ranges);
    ranges.iter().map(persisted_fold_range).collect()
}

fn persisted_fold_range(range: &FoldedRange) -> PersistedFoldRange {
    PersistedFoldRange {
        start_line: range.start_line,
        end_line: range.end_line,
    }
}

pub(crate) fn folded_ranges_from_session(
    states: &[BufferFoldState],
) -> HashMap<PathBuf, Vec<FoldedRange>> {
    let mut folded = HashMap::<PathBuf, Vec<FoldedRange>>::new();
    for state in states {
        if !has_session_path_identity(&state.path) {
            continue;
        }

        let mut ranges_from_state = state
            .ranges
            .iter()
            .filter_map(folded_range_from_persisted)
            .peekable();
        if ranges_from_state.peek().is_none() {
            continue;
        }

        let ranges = folded.entry(state.path.clone()).or_default();
        ranges.extend(ranges_from_state);
    }
    for ranges in folded.values_mut() {
        normalize_session_fold_ranges(ranges);
    }
    folded.retain(|_, ranges| !ranges.is_empty());
    folded
}

fn folded_range_from_persisted(range: &PersistedFoldRange) -> Option<FoldedRange> {
    let range = FoldedRange {
        start_line: range.start_line,
        end_line: range.end_line,
    };
    is_valid_session_fold_range(&range).then_some(range)
}

pub(crate) fn clamp_folded_ranges_for_line_count(
    folded_ranges: &mut HashMap<PathBuf, Vec<FoldedRange>>,
    path: &Path,
    line_count: usize,
) {
    if line_count < 2 {
        folded_ranges.remove(path);
        return;
    }

    let Some(ranges) = folded_ranges.get_mut(path) else {
        return;
    };
    clamp_session_fold_ranges_for_line_count(ranges, line_count);
    if ranges.is_empty() {
        folded_ranges.remove(path);
    }
}

fn clamp_session_fold_ranges_for_line_count(ranges: &mut Vec<FoldedRange>, line_count: usize) {
    ranges.retain_mut(|range| {
        if range.start_line == 0 || range.start_line >= line_count {
            return false;
        }
        range.end_line = range.end_line.min(line_count);
        range.end_line > range.start_line
    });
    normalize_session_fold_ranges(ranges);
}

fn normalize_session_fold_ranges(ranges: &mut Vec<FoldedRange>) {
    normalize_folded_ranges(ranges);
    coalesce_session_fold_ranges_sharing_start(ranges);
    discard_crossing_session_fold_ranges(ranges);
}

fn coalesce_session_fold_ranges_sharing_start(ranges: &mut Vec<FoldedRange>) {
    if ranges.len() < 2 {
        return;
    }

    let mut coalesced: Vec<FoldedRange> = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        if let Some(previous) = coalesced.last_mut()
            && previous.start_line == range.start_line
        {
            previous.end_line = previous.end_line.max(range.end_line);
            continue;
        }
        coalesced.push(range);
    }
    *ranges = coalesced;
}

fn discard_crossing_session_fold_ranges(ranges: &mut Vec<FoldedRange>) {
    if ranges.len() < 2 {
        return;
    }

    let mut active_ends = Vec::<usize>::new();
    let mut retained = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        while active_ends
            .last()
            .is_some_and(|end_line| *end_line < range.start_line)
        {
            active_ends.pop();
        }
        if active_ends
            .last()
            .is_some_and(|end_line| range.end_line > *end_line)
        {
            continue;
        }
        active_ends.push(range.end_line);
        retained.push(range);
    }
    *ranges = retained;
}

fn is_valid_session_fold_range(range: &FoldedRange) -> bool {
    range.start_line > 0 && range.end_line > range.start_line
}

fn has_session_path_identity(path: &Path) -> bool {
    !path.as_os_str().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{BufferFoldState, PersistedFoldRange};

    #[test]
    fn restore_skips_invalid_session_ranges_before_group_normalization() {
        let path = PathBuf::from("workspace/src/main.rs");
        let states = vec![BufferFoldState {
            path: path.clone(),
            ranges: vec![
                PersistedFoldRange {
                    start_line: 0,
                    end_line: 3,
                },
                PersistedFoldRange {
                    start_line: 7,
                    end_line: 7,
                },
                PersistedFoldRange {
                    start_line: 2,
                    end_line: 5,
                },
            ],
        }];

        let restored = folded_ranges_from_session(&states);

        assert_eq!(
            restored.get(&path),
            Some(&vec![FoldedRange {
                start_line: 2,
                end_line: 5,
            }])
        );
    }

    #[test]
    fn restore_canonicalizes_ambiguous_and_crossing_session_ranges() {
        let path = PathBuf::from("workspace/src/main.rs");
        let states = vec![BufferFoldState {
            path: path.clone(),
            ranges: vec![
                PersistedFoldRange {
                    start_line: 2,
                    end_line: 5,
                },
                PersistedFoldRange {
                    start_line: 2,
                    end_line: 8,
                },
                PersistedFoldRange {
                    start_line: 4,
                    end_line: 6,
                },
                PersistedFoldRange {
                    start_line: 6,
                    end_line: 10,
                },
                PersistedFoldRange {
                    start_line: 9,
                    end_line: 11,
                },
            ],
        }];

        let restored = folded_ranges_from_session(&states);

        assert_eq!(
            restored.get(&path),
            Some(&vec![
                FoldedRange {
                    start_line: 2,
                    end_line: 8,
                },
                FoldedRange {
                    start_line: 4,
                    end_line: 6,
                },
                FoldedRange {
                    start_line: 9,
                    end_line: 11,
                },
            ])
        );
    }

    #[test]
    fn session_save_canonicalizes_ambiguous_and_crossing_ranges() {
        let path = PathBuf::from("workspace/src/main.rs");
        let folded = HashMap::from([(
            path.clone(),
            vec![
                FoldedRange {
                    start_line: 4,
                    end_line: 6,
                },
                FoldedRange {
                    start_line: 2,
                    end_line: 5,
                },
                FoldedRange {
                    start_line: 2,
                    end_line: 8,
                },
                FoldedRange {
                    start_line: 6,
                    end_line: 10,
                },
                FoldedRange {
                    start_line: 9,
                    end_line: 11,
                },
            ],
        )]);

        let states = session_fold_states(&folded);

        assert_eq!(
            states,
            vec![BufferFoldState {
                path,
                ranges: vec![
                    PersistedFoldRange {
                        start_line: 2,
                        end_line: 8,
                    },
                    PersistedFoldRange {
                        start_line: 4,
                        end_line: 6,
                    },
                    PersistedFoldRange {
                        start_line: 9,
                        end_line: 11,
                    },
                ],
            }]
        );
    }

    #[test]
    fn clamp_removes_session_folds_when_loaded_file_cannot_fold() {
        let path = PathBuf::from("workspace/src/main.rs");
        let mut folded = HashMap::from([(
            path.clone(),
            vec![FoldedRange {
                start_line: 1,
                end_line: 3,
            }],
        )]);

        clamp_folded_ranges_for_line_count(&mut folded, &path, 1);

        assert!(!folded.contains_key(&path));
    }

    #[test]
    fn clamp_deduplicates_ranges_that_collapse_to_same_loaded_span() {
        let path = PathBuf::from("workspace/src/main.rs");
        let mut folded = HashMap::from([(
            path.clone(),
            vec![
                FoldedRange {
                    start_line: 2,
                    end_line: 8,
                },
                FoldedRange {
                    start_line: 2,
                    end_line: 12,
                },
            ],
        )]);

        clamp_folded_ranges_for_line_count(&mut folded, &path, 5);

        assert_eq!(
            folded.get(&path),
            Some(&vec![FoldedRange {
                start_line: 2,
                end_line: 5,
            }])
        );
    }

    #[test]
    fn clamp_removes_session_folds_starting_at_or_after_loaded_line_count() {
        let path = PathBuf::from("workspace/src/main.rs");
        let mut folded = HashMap::from([(
            path.clone(),
            vec![
                FoldedRange {
                    start_line: 4,
                    end_line: 8,
                },
                FoldedRange {
                    start_line: 5,
                    end_line: 9,
                },
                FoldedRange {
                    start_line: 6,
                    end_line: 12,
                },
            ],
        )]);

        clamp_folded_ranges_for_line_count(&mut folded, &path, 5);

        assert_eq!(
            folded.get(&path),
            Some(&vec![FoldedRange {
                start_line: 4,
                end_line: 5,
            }])
        );
    }

    #[test]
    fn clamp_removes_invalid_out_of_buffer_and_crossing_ranges() {
        let path = PathBuf::from("workspace/src/main.rs");
        let mut folded = HashMap::from([(
            path.clone(),
            vec![
                FoldedRange {
                    start_line: 0,
                    end_line: 9,
                },
                FoldedRange {
                    start_line: 2,
                    end_line: 5,
                },
                FoldedRange {
                    start_line: 4,
                    end_line: 12,
                },
                FoldedRange {
                    start_line: 6,
                    end_line: 8,
                },
            ],
        )]);

        clamp_folded_ranges_for_line_count(&mut folded, &path, 6);

        assert_eq!(
            folded.get(&path),
            Some(&vec![FoldedRange {
                start_line: 2,
                end_line: 5,
            }])
        );
    }
}
