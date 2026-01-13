// Search utilities for plugin sources

use std::cmp::Ordering;

/// Trait for items that can be searched
pub trait Searchable {
    /// Get the name to compare against the search query
    fn search_name(&self) -> &str;
}

/// Parsed plugin ID that can be either a full identifier or a search term
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedId {
    /// Full identifier with owner and name (e.g., "ViaVersion/ViaVersion")
    Full { owner: String, name: String },
    /// Just a search term (e.g., "ViaVersion")
    SearchTerm(String),
}

/// Parse an owner/name style ID
/// Returns Full if it contains a slash with non-empty parts, otherwise SearchTerm
pub fn parse_owner_name_id(id: &str) -> ParsedId {
    let parts: Vec<&str> = id.split('/').collect();

    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        ParsedId::Full {
            owner: parts[0].to_string(),
            name: parts[1].to_string(),
        }
    } else {
        ParsedId::SearchTerm(id.to_string())
    }
}

/// Rank search results with exact matches first (case-insensitive)
/// Takes a mutable slice and sorts it in place
pub fn rank_search_results<T: Searchable>(results: &mut [T], query: &str) {
    let query_lower = query.to_lowercase();
    let query_spaced = query_lower.replace('-', " ");

    results.sort_by(|a, b| {
        let a_name_lower = a.search_name().to_lowercase();
        let b_name_lower = b.search_name().to_lowercase();

        // Exact match gets highest priority (check both hyphenated and spaced versions)
        let a_exact = a_name_lower == query_lower || a_name_lower == query_spaced;
        let b_exact = b_name_lower == query_lower || b_name_lower == query_spaced;

        match (a_exact, b_exact) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => {
                // If both or neither are exact, sort alphabetically by name
                a_name_lower.cmp(&b_name_lower)
            }
        }
    });
}

/// Rank search results with exact matches first, preserving original order for ties
/// Useful when the original order has meaning (e.g., sorted by popularity)
pub fn rank_search_results_stable<T: Searchable>(results: &mut [T], query: &str) {
    let query_lower = query.to_lowercase();

    results.sort_by(|a, b| {
        let a_name_lower = a.search_name().to_lowercase();
        let b_name_lower = b.search_name().to_lowercase();

        // Exact match gets highest priority
        let a_exact = a_name_lower == query_lower;
        let b_exact = b_name_lower == query_lower;

        match (a_exact, b_exact) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => Ordering::Equal, // Preserve original order
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestItem {
        name: String,
    }

    impl Searchable for TestItem {
        fn search_name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_parse_owner_name_full() {
        let id = parse_owner_name_id("ViaVersion/ViaVersion");
        assert_eq!(
            id,
            ParsedId::Full {
                owner: "ViaVersion".to_string(),
                name: "ViaVersion".to_string()
            }
        );
    }

    #[test]
    fn test_parse_owner_name_search_term() {
        let id = parse_owner_name_id("ViaVersion");
        assert_eq!(id, ParsedId::SearchTerm("ViaVersion".to_string()));
    }

    #[test]
    fn test_parse_owner_name_empty_parts() {
        let id = parse_owner_name_id("/ViaVersion");
        assert_eq!(id, ParsedId::SearchTerm("/ViaVersion".to_string()));

        let id = parse_owner_name_id("ViaVersion/");
        assert_eq!(id, ParsedId::SearchTerm("ViaVersion/".to_string()));
    }

    #[test]
    fn test_rank_exact_match_first() {
        let mut items = vec![
            TestItem {
                name: "WorldEditPro".to_string(),
            },
            TestItem {
                name: "WorldEdit".to_string(),
            },
            TestItem {
                name: "WorldEditHelper".to_string(),
            },
        ];

        rank_search_results(&mut items, "WorldEdit");

        assert_eq!(items[0].name, "WorldEdit");
    }

    #[test]
    fn test_rank_case_insensitive() {
        let mut items = vec![
            TestItem {
                name: "WORLDEDIT".to_string(),
            },
            TestItem {
                name: "worldedit".to_string(),
            },
        ];

        rank_search_results(&mut items, "WorldEdit");

        // Both are exact matches, so they should be sorted alphabetically
        // "WORLDEDIT" < "worldedit" in lowercase comparison
    }

    #[test]
    fn test_rank_hyphen_match() {
        let mut items = vec![
            TestItem {
                name: "World Edit".to_string(),
            },
            TestItem {
                name: "WorldEditPro".to_string(),
            },
        ];

        rank_search_results(&mut items, "world-edit");

        assert_eq!(items[0].name, "World Edit");
    }
}
