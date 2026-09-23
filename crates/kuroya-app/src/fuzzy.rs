use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

/// Smart-case-aware fuzzy match.
///
/// `SkimMatcherV2::default()` turns case-sensitive as soon as the query
/// contains an uppercase character, so uppercase acronyms ("SCM", "GIT",
/// "SRC") silently match nothing against lowercase candidates. This runs the
/// normal pass first, and when it finds nothing and the query does contain
/// uppercase, retries with a lowercased query.
pub(crate) fn fuzzy_match_with_case_fallback(
    matcher: &SkimMatcherV2,
    candidate: &str,
    query: &str,
) -> Option<i64> {
    matcher.fuzzy_match(candidate, query).or_else(|| {
        let has_uppercase = query.chars().any(|c| c.is_ascii_uppercase());
        if !has_uppercase {
            return None;
        }
        let lowered = query.to_ascii_lowercase();
        matcher.fuzzy_match(candidate, &lowered)
    })
}

#[cfg(test)]
mod tests {
    use super::fuzzy_match_with_case_fallback;
    use fuzzy_matcher::FuzzyMatcher;
    use fuzzy_matcher::skim::SkimMatcherV2;

    #[test]
    fn uppercase_query_falls_back_to_case_insensitive() {
        let matcher = SkimMatcherV2::default();
        assert_eq!(
            matcher.fuzzy_match("src/localization/app.rs", "SRC"),
            None,
            "smart case must reject the plain pass for the test to be meaningful"
        );
        assert!(
            fuzzy_match_with_case_fallback(&matcher, "src/localization/app.rs", "SRC").is_some()
        );
    }

    #[test]
    fn lowercase_query_behaves_like_the_plain_matcher() {
        let matcher = SkimMatcherV2::default();
        assert_eq!(
            fuzzy_match_with_case_fallback(&matcher, "src/main.rs", "src"),
            matcher.fuzzy_match("src/main.rs", "src")
        );
        assert_eq!(
            fuzzy_match_with_case_fallback(&matcher, "src/main.rs", "zzz"),
            None
        );
    }
}
