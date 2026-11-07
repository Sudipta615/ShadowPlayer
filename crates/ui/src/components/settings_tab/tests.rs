//! Settings search, index reflection, and rendering-coverage tests.
//!
//! These stay out of `mod.rs` so the façade is only the tab shell and the
//! category dispatch. The `include_str!` paths resolve against this
//! directory, so they read the same sources the panel renders.

#[cfg(test)]
mod search_tests {
    use crate::components::settings_tab::*;
    use std::collections::BTreeMap;

    /// Every source that renders a settings row, paired with the category it
    /// belongs to. The chain-preset card is rendered inside the Spatial
    /// category, so its rows are checked as Spatial rows.
    fn category_sources() -> [(SettingsCategory, String); 8] {
        [
            (
                SettingsCategory::Playback,
                include_str!("categories/playback.rs").to_string(),
            ),
            (
                SettingsCategory::Output,
                include_str!("categories/output.rs").to_string(),
            ),
            (
                SettingsCategory::Spatial,
                [
                    include_str!("categories/spatial.rs"),
                    include_str!("cards/chain_presets.rs"),
                ]
                .concat(),
            ),
            (
                SettingsCategory::MixBus,
                include_str!("categories/mixbus.rs").to_string(),
            ),
            (
                SettingsCategory::Tools,
                include_str!("categories/tools.rs").to_string(),
            ),
            (
                SettingsCategory::Converter,
                include_str!("categories/converter.rs").to_string(),
            ),
            (
                SettingsCategory::Library,
                include_str!("categories/library.rs").to_string(),
            ),
            (
                SettingsCategory::Appearance,
                include_str!("categories/appearance.rs").to_string(),
            ),
        ]
    }

    /// Parse a Rust string literal at the start of `text`.
    fn string_literal(text: &str) -> Option<(String, usize)> {
        let mut chars = text.char_indices();
        let (_, quote) = chars.next()?;
        if quote != '"' {
            return None;
        }
        let mut out = String::new();
        while let Some((index, c)) = chars.next() {
            match c {
                '"' => return Some((out, index + 1)),
                '\\' => {
                    let (_, escaped) = chars.next()?;
                    out.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    });
                }
                _ => out.push(c),
            }
        }
        None
    }

    /// Parse the label of a settings row.
    ///
    /// The label is either a plain literal or a translated one: the copy is
    /// in the catalogue, so the render says `&tr!("Label")` while the index
    /// still keys on the English text. Both spellings name the same row, and
    /// the drift test must see through the translation to prove that.
    fn row_label_literal(text: &str) -> Option<(String, usize)> {
        let mut rest = text.trim_start();
        if let Some(stripped) = rest.strip_prefix('&') {
            rest = stripped.trim_start();
        }
        if let Some(stripped) = rest.strip_prefix("tr!(") {
            return string_literal(stripped.trim_start());
        }
        string_literal(rest)
    }

    /// Every `setting_item(ui, "Label", ...)` call and its byte offset.
    ///
    /// A call whose first argument is not `ui` followed by a string literal is
    /// the definition dispatching to `setting_item_with_info`, not a row.
    fn rendered_rows(source: &str) -> Vec<(usize, String)> {
        let mut rows = Vec::new();
        for name in [
            "setting_item(",
            "setting_item_with_info(",
            "setting_item_toggle(",
        ] {
            let mut from = 0;
            while let Some(offset) = source[from..].find(name) {
                let start = from + offset;
                let after = start + name.len();
                let rest = source[after..].trim_start();
                let rest = rest.strip_prefix("ui").unwrap_or("");
                let rest = rest.trim_start();
                let rest = rest.strip_prefix(',').unwrap_or("");
                let rest = rest.trim_start();
                if let Some((label, _)) = row_label_literal(rest) {
                    rows.push((start, label));
                }
                from = after;
            }
        }

        // The diagnostics card renders one row per `OutputFact` in a loop.
        // The labels are static by design; list them here so the search index
        // remains addressable without pretending the runtime list is
        // discoverable by parsing a `for` body.
        // `rustfmt` wraps the iterator expression onto its own line, so search
        // for the call rather than for a single-line `for … in …` spelling.
        if let Some(at) = source.find("crate::components::output_facts::output_facts(") {
            for label in [
                "Requested backend",
                "Negotiated backend",
                "Access mode",
                "Verification",
                "Output device",
                "Negotiated rate / container",
                "Period / buffer",
            ] {
                rows.push((at, label.to_string()));
            }
        }
        rows.sort_by_key(|(at, _)| *at);
        rows
    }

    /// Every card title passed to `card_header*`.
    fn rendered_cards(source: &str) -> Vec<String> {
        let mut cards = Vec::new();
        for name in ["card_header(", "card_header_with_info("] {
            let mut from = 0;
            while let Some(offset) = source[from..].find(name) {
                let after = from + offset + name.len();
                // A real call starts `ui,`; the definition starts `ui:`.
                let rest = source[after..].trim_start();
                if let Some(rest) = rest.strip_prefix("ui,") {
                    // `ui` and the icon contain no string literals, so the
                    // first literal is always the title.
                    if let Some(quote) = rest.find('"') {
                        if let Some((title, _)) = string_literal(&rest[quote..]) {
                            cards.push(title);
                        }
                    }
                }
                from = after;
            }
        }
        cards
    }

    #[test]
    fn every_rendered_setting_is_indexed() {
        let sources = category_sources();
        let indexed: std::collections::BTreeSet<&str> =
            SETTINGS_INDEX.iter().map(|entry| entry.label).collect();
        let mut rendered_count = 0;
        for (_, source) in &sources {
            for (at, label) in rendered_rows(source) {
                rendered_count += 1;
                assert!(
                    indexed.contains(label.as_str()),
                    "`{label}` is rendered at byte {at} but missing from SETTINGS_INDEX"
                );
            }
        }
        assert_eq!(
            rendered_count,
            SETTINGS_INDEX.len(),
            "the render has {rendered_count} setting rows, the index has {}",
            SETTINGS_INDEX.len()
        );
    }

    #[test]
    fn every_indexed_setting_is_rendered_once() {
        let mut rendered: BTreeMap<String, usize> = BTreeMap::new();
        for (_, source) in &category_sources() {
            for (_, label) in rendered_rows(source) {
                *rendered.entry(label).or_default() += 1;
            }
        }
        let mut indexed: BTreeMap<&str, usize> = BTreeMap::new();
        for entry in SETTINGS_INDEX.iter() {
            *indexed.entry(entry.label).or_default() += 1;
        }
        for (label, count) in &indexed {
            assert_eq!(
                rendered.get(*label).copied().unwrap_or(0),
                *count,
                "`{label}` has {count} index rows but {} rendered rows",
                rendered.get(*label).copied().unwrap_or(0)
            );
        }
        for (label, count) in &rendered {
            assert_eq!(
                count, &1,
                "`{label}` is rendered {count} times; the index cannot address one of them"
            );
        }
    }

    #[test]
    fn the_index_carries_the_right_category_and_card() {
        let sources = category_sources();
        let mut cards = Vec::new();
        for (_, source) in &sources {
            cards.extend(rendered_cards(source));
        }
        let card_set: std::collections::BTreeSet<&str> = cards.iter().map(String::as_str).collect();
        let indexed_cards: std::collections::BTreeSet<&str> =
            SETTINGS_INDEX.iter().map(|entry| entry.card).collect();
        assert!(
            indexed_cards.is_subset(&card_set),
            "the index names cards the page does not render: {:?}",
            indexed_cards.difference(&card_set).collect::<Vec<_>>()
        );

        for (category, source) in &sources {
            for (at, label) in rendered_rows(source) {
                let entry = SETTINGS_INDEX
                    .iter()
                    .find(|entry| entry.label == label)
                    .unwrap_or_else(|| panic!("`{label}` is not indexed"));
                assert_eq!(
                    entry.category, *category,
                    "`{label}` is indexed under {:?} but rendered under {category:?} at byte {at}",
                    entry.category
                );
                assert!(
                    cards.iter().any(|card| card == entry.card),
                    "`{label}` names the card `{}`, which the page does not render",
                    entry.card
                );
            }
        }
    }

    #[test]
    fn search_reaches_card_headings() {
        let hits = crate::components::settings_tab::index::scored_setting_cards("theme editor");
        assert!(
            hits.iter().any(
                |(_, category, title)| *category == SettingsCategory::Appearance
                    && *title == "Theme Editor"
            ),
            "the Theme Editor card must be searchable: {hits:?}"
        );
        let hits = crate::components::settings_tab::index::scored_setting_cards("theme gallery");
        assert!(
            hits.iter().any(
                |(_, category, title)| *category == SettingsCategory::Appearance
                    && *title == "Theme Gallery"
            ),
            "the Theme Gallery card must be searchable: {hits:?}"
        );
    }

    #[test]
    fn search_reaches_labels_cards_and_descriptions() {
        assert_eq!(
            search_settings("subwoofer")
                .first()
                .map(|entry| entry.label),
            Some("Subwoofer Phase"),
            "a label match outranks a description match"
        );
        assert!(
            search_settings("crossfeed")
                .iter()
                .any(|entry| entry.label == "Processing Profile"),
            "a description match still finds the row: {:?}",
            search_settings("crossfeed")
                .iter()
                .map(|entry| entry.label)
                .collect::<Vec<_>>()
        );
        assert!(
            search_settings("no such setting exists").is_empty(),
            "a query with no matched characters finds nothing"
        );
        assert!(search_settings("   ").is_empty());
    }

    #[test]
    fn every_index_entry_can_be_found_by_its_own_label() {
        for entry in SETTINGS_INDEX.iter() {
            let results = search_settings(entry.label);
            assert!(
                results.iter().any(|found| found.label == entry.label),
                "`{}` is not found by its own label",
                entry.label
            );
        }
    }
}
