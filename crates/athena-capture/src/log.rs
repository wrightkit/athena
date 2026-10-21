/// One Inspector log line, `[hh:mm:ss] message`.
pub struct Line<'a> {
    pub raw: &'a str,
    pub message: &'a str,
}

pub fn lines(text: &str) -> Vec<Line<'_>> {
    text.lines()
        .filter_map(|raw| {
            let b = raw.as_bytes();
            let stamped = b.len() > 11
                && b[0] == b'['
                && b[3] == b':'
                && b[6] == b':'
                && b[9] == b']'
                && b[10] == b' '
                && [1, 2, 4, 5, 7, 8].iter().all(|&i| b[i].is_ascii_digit());
            stamped.then(|| Line {
                raw,
                message: &raw[11..],
            })
        })
        .collect()
}
