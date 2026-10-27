//! Rules for saved searches.

use thiserror::Error;

use crate::query::{QueryError, parse_query};

pub const MAX_SEARCH_NAME_CHARS: usize = 64;
pub const MAX_SAVED_QUERY_CHARS: usize = 1000;
pub const MAX_SAVED_SEARCHES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SavedSearchError {
    #[error("give the search a name")]
    EmptyName,
    #[error("the name is longer than {MAX_SEARCH_NAME_CHARS} characters")]
    NameTooLong,
    #[error("the name contains a control character")]
    ControlChar,
    #[error("the search is empty")]
    EmptyQuery,
    #[error("the search is longer than {MAX_SAVED_QUERY_CHARS} characters")]
    QueryTooLong,
    #[error(transparent)]
    Query(#[from] QueryError),
}

/// Trims the name and query and checks both. The query must parse.
pub fn clean_saved_search(name: &str, query: &str) -> Result<(String, String), SavedSearchError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(SavedSearchError::EmptyName);
    }
    if name.chars().count() > MAX_SEARCH_NAME_CHARS {
        return Err(SavedSearchError::NameTooLong);
    }
    if name.chars().any(char::is_control) {
        return Err(SavedSearchError::ControlChar);
    }
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        return Err(SavedSearchError::EmptyQuery);
    }
    if query.chars().count() > MAX_SAVED_QUERY_CHARS {
        return Err(SavedSearchError::QueryTooLong);
    }
    parse_query(&query)?;
    Ok((name, query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_names_and_queries() {
        assert_eq!(
            clean_saved_search("  Blue   skies ", " blue_sky  -cloud "),
            Ok(("Blue skies".into(), "blue_sky -cloud".into()))
        );
        assert_eq!(
            clean_saved_search(" ", "a"),
            Err(SavedSearchError::EmptyName)
        );
        assert_eq!(
            clean_saved_search("x", "  "),
            Err(SavedSearchError::EmptyQuery)
        );
        assert_eq!(
            clean_saved_search(&"n".repeat(65), "a"),
            Err(SavedSearchError::NameTooLong)
        );
        assert_eq!(
            clean_saved_search("a\u{7}b", "a"),
            Err(SavedSearchError::ControlChar)
        );
        assert_eq!(
            clean_saved_search("x", &"a".repeat(1001)),
            Err(SavedSearchError::QueryTooLong)
        );
        assert!(matches!(
            clean_saved_search("x", &"a ".repeat(100)),
            Err(SavedSearchError::Query(_))
        ));
    }
}
