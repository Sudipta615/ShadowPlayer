pub(crate) fn tag_contents<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let lower = xml.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(&open) {
        let start = from + rel;
        let after_name = start + open.len();
        // `<trackList>` must not match `<track>`.
        if lower[after_name..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            from = after_name;
            continue;
        }
        let Some(gt) = lower[after_name..].find('>') else {
            break;
        };
        let content_start = after_name + gt + 1;
        // Self-closing elements have no contents.
        if lower[after_name..content_start].trim_end().ends_with('/') {
            from = content_start;
            continue;
        }
        let Some(close_rel) = lower[content_start..].find(&close) else {
            break;
        };
        let content_end = content_start + close_rel;
        out.push(&xml[content_start..content_end]);
        from = content_end + close.len();
    }
    out
}

/// Every opening tag named `tag`, as the text from `<` through `>`.
pub(crate) fn all_tags<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let lower = xml.to_ascii_lowercase();
    let open = format!("<{tag}");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(&open) {
        let start = from + rel;
        let after_name = start + open.len();
        if lower[after_name..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            from = after_name;
            continue;
        }
        let Some(gt) = lower[after_name..].find('>') else {
            break;
        };
        let end = after_name + gt + 1;
        out.push(&xml[start..end]);
        from = end;
    }
    out
}

pub(crate) fn first_attr(tag: &str, element: &str, attribute: &str) -> Option<String> {
    all_tags(tag, element)
        .into_iter()
        .find_map(|t| attr(t, attribute))
}

pub(crate) fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(&needle) {
        let start = from + rel;
        // A window function must match the whole attribute name.
        let before_ok = start == 0
            || lower[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_whitespace());
        let after = start + needle.len();
        let after_ok = lower[after..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_whitespace() || c == '=');
        if !before_ok || !after_ok {
            from = after;
            continue;
        }
        let mut cursor = after;
        while lower[cursor..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            cursor += lower[cursor..].chars().next()?.len_utf8();
        }
        if !lower[cursor..].starts_with('=') {
            from = after;
            continue;
        }
        cursor += 1;
        while lower[cursor..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            cursor += lower[cursor..].chars().next()?.len_utf8();
        }
        let quote = tag[cursor..].chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value_start = cursor + quote.len_utf8();
        let value_rest = &tag[value_start..];
        let value_end = value_rest.find(quote)?;
        return Some(decode_xml_entities(&value_rest[..value_end]));
    }
    None
}

pub(crate) fn decode_xml_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        let Some(semi) = after.find(';') else {
            out.push_str(after);
            return out;
        };
        let entity = &after[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&after[..=semi]),
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    out
}
