#[cfg(test)]
mod tests {
    use crate::parser::*;

    #[test]
    fn test_parse_lrc_basic() {
        let lrc = r#"
[ti:Midnight Dreams]
[ar:Horizon Lines]
[00:12.50]Dancing in the neon light
[00:18.00]Shadows whispering through the night
[00:25.30]Midnight dreams take flight
"#;

        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].start_time_secs, 12.5);
        assert_eq!(lines[0].text, "Dancing in the neon light");
        assert_eq!(lines[1].start_time_secs, 18.0);
        assert_eq!(lines[2].start_time_secs, 25.3);
    }

    #[test]
    fn test_find_active_line() {
        let lrc = r#"
[00:10.00]First line
[00:20.00]Second line
[00:30.00]Third line
"#;
        let lines = parse_lrc(lrc);

        assert_eq!(find_active_line(&lines, 5.0), None);
        assert_eq!(find_active_line(&lines, 10.0), Some(0));
        assert_eq!(find_active_line(&lines, 15.0), Some(0));
        assert_eq!(find_active_line(&lines, 20.0), Some(1));
        assert_eq!(find_active_line(&lines, 25.0), Some(1));
        assert_eq!(find_active_line(&lines, 30.0), Some(2));
        assert_eq!(find_active_line(&lines, 99.0), Some(2));
    }
}
