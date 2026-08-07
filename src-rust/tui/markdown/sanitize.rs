pub(super) fn clean(text: &str) -> String {
    let normalized = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ");
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut clean = String::with_capacity(normalized.len());
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\u{1b}' {
            index = skip_escape(&chars, index + 1);
            continue;
        }
        if ch == '\n' || !ch.is_control() {
            clean.push(ch);
        }
        index += 1;
    }
    clean
}

fn skip_escape(chars: &[char], mut index: usize) -> usize {
    let Some(&kind) = chars.get(index) else {
        return index;
    };
    index += 1;
    match kind {
        '[' => {
            while let Some(&ch) = chars.get(index) {
                index += 1;
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        }
        ']' => {
            while let Some(&ch) = chars.get(index) {
                index += 1;
                if ch == '\u{7}' {
                    break;
                }
                if ch == '\u{1b}' && chars.get(index) == Some(&'\\') {
                    index += 1;
                    break;
                }
            }
        }
        'P' | 'X' | '^' | '_' => {
            while let Some(&ch) = chars.get(index) {
                index += 1;
                if ch == '\u{1b}' && chars.get(index) == Some(&'\\') {
                    index += 1;
                    break;
                }
            }
        }
        _ => {}
    }
    index
}
