//! Bounded custom-source loading and positive process-tree predicate checks.
//!
//! This checks probe activation, not arbitrary action safety: custom BPFTrace
//! programs remain trusted code. Opaque preprocessor expansion is rejected.

use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

pub(super) fn read(path: &Path, parameters: &[(String, String)]) -> Result<String, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("cannot read bpftrace program {}: {error}", path.display()))?;
    let metadata = file.metadata().map_err(|error| {
        format!(
            "cannot inspect bpftrace program {}: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "bpftrace program {} is not a regular file",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    file.take(super::MAX_CUSTOM_PROGRAM + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read bpftrace program {}: {error}", path.display()))?;
    if bytes.len() as u64 > super::MAX_CUSTOM_PROGRAM {
        return Err(format!(
            "bpftrace program {} is too large; limit {} bytes",
            path.display(),
            super::MAX_CUSTOM_PROGRAM
        ));
    }
    let source = String::from_utf8(bytes)
        .map_err(|_| format!("bpftrace program {} is not UTF-8", path.display()))?;
    let source = bind_parameters(&source, parameters)
        .map_err(|reason| format!("bpftrace program {}: {reason}", path.display()))?;
    validate(&source).map_err(|reason| format!("bpftrace program {} {reason}", path.display()))?;
    Ok(source)
}

pub(super) fn parameter(key: &str, value: &str) -> Result<(), String> {
    let valid_key = key.strip_prefix("ASB_PARAM_").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.as_bytes()[0].is_ascii_uppercase()
            && key.len() <= 64
            && suffix
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    });
    if !valid_key {
        return Err("invalid bpftrace parameter name; use ASB_PARAM_LIMIT=12 (ASB_PARAM_ followed by an uppercase identifier, at most 64 characters). Legacy names such as limit must be renamed in the program and configuration".into());
    }
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|b| b.is_ascii_digit())
        || value
            .parse::<u64>()
            .ok()
            .is_none_or(|n| n > i64::MAX as u64)
    {
        return Err(format!(
            "invalid bpftrace parameter {key}: use a canonical decimal integer from 0 through {}",
            i64::MAX
        ));
    }
    Ok(())
}

/// Bind only raw identifier tokens, never strings, comments, maps or variables.
/// Numeric literals cannot inject operators, directives, probes or memberships.
fn bind_parameters(source: &str, parameters: &[(String, String)]) -> Result<String, String> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut bindings = BTreeMap::new();
    for (key, value) in parameters {
        parameter(key, value)?;
        if bindings.insert(key.as_str(), value.as_str()).is_some() {
            return Err(format!(
                "duplicate bpftrace parameter {key}; specify each parameter once"
            ));
        }
    }
    let code = binding_mask(&code_mask(source).map_err(str::to_owned)?);
    let bytes = code.as_bytes();
    let mut used = BTreeSet::new();
    let mut result = String::new();
    let mut copied = 0;
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_alphabetic() && bytes[index] != b'_' {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        let token = &code[start..index];
        let preceding = code[..start].trim_end();
        if !token.starts_with("ASB_PARAM_")
            || preceding.ends_with(['@', '$', '.'])
            || preceding.ends_with("->")
        {
            continue;
        }
        let value = bindings.get(token).ok_or_else(|| {
            format!("missing bpftrace parameter {token}; provide PROGRAM?{token}=12")
        })?;
        result.push_str(&source[copied..start]);
        result.push_str(value);
        copied = index;
        used.insert(token);
    }
    result.push_str(&source[copied..]);
    for key in bindings.keys() {
        if !used.contains(key) {
            return Err(format!(
                "bpftrace parameter {key} is not referenced as an identifier in the program"
            ));
        }
    }
    Ok(result)
}

/// Probe provider/path/symbol fields are attachment tokens, not expressions.
/// Preserve their exact source bytes even when an unquoted path component or
/// symbol happens to have the same spelling as a numeric parameter.
fn binding_mask(code: &str) -> String {
    let mut mask = code.as_bytes().to_vec();
    let mut depth = 0usize;
    let mut header_start = 0;
    for (index, byte) in code.bytes().enumerate() {
        if byte == b'{' {
            if depth == 0 {
                let header = &code[header_start..index];
                let header = header.trim_start_matches(|c: char| c.is_whitespace() || c == ';');
                let start = index - header.len();
                let before_predicate = header.trim_end().strip_suffix('/').and_then(|body| {
                    body.match_indices('/').take(64).find_map(|(open, _)| {
                        (attachments(&body[..open]) == Some(false)).then_some(open + 1)
                    })
                });
                let count = before_predicate.or_else(|| attachments(header).map(|_| header.len()));
                if let Some(count) = count {
                    mask[start..start + count].fill(b' ');
                }
            }
            depth += 1;
        } else if byte == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                header_start = index + 1;
            }
        }
    }
    // Only ASCII attachment bytes and already-masked strings are replaced.
    String::from_utf8(mask).expect("mask preserves UTF-8 boundaries")
}

/// Replace comments/strings with spaces, retaining structural byte offsets.
fn code_mask(source: &str) -> Result<String, &'static str> {
    let bytes = source.as_bytes();
    let mut masked = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        let start = index;
        if matches!(bytes[index], b'\'' | b'"') {
            let quote = bytes[index];
            index += 1;
            while index < bytes.len() && bytes[index] != quote {
                if bytes[index] == b'\\' {
                    index += 1;
                }
                index += 1;
            }
            if index >= bytes.len() {
                return Err("contains an unterminated string");
            }
            index += 1;
        } else if bytes[index..].starts_with(b"//") {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"/*") {
            index += 2;
            while index + 1 < bytes.len() && !bytes[index..].starts_with(b"*/") {
                index += 1;
            }
            if index + 1 >= bytes.len() {
                return Err("contains an unterminated comment");
            }
            index += 2;
        } else {
            index += 1;
            continue;
        }
        for byte in &mut masked[start..index] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(masked).map_err(|_| "contains invalid source encoding")
}

fn validate(source: &str) -> Result<(), &'static str> {
    let code = code_mask(source)?;
    if code.contains('#') {
        return Err(
            "uses opaque preprocessor directives; inline declarations so every probe can be checked for process-tree scope",
        );
    }
    let mut remaining = code.as_str();
    let mut saw_probe = false;
    while !remaining
        .trim_matches(|c: char| c.is_whitespace() || c == ';')
        .is_empty()
    {
        remaining = remaining.trim_start_matches(|c: char| c.is_whitespace() || c == ';');
        let open = remaining
            .find('{')
            .ok_or("contains a probe or declaration without an action block")?;
        let header = remaining[..open].trim();
        if header.len() > 8192 {
            return Err(
                "has a probe header exceeding 8192 bytes; use simpler attachments and predicates",
            );
        }
        let mut depth = 1;
        let mut close = None;
        for (offset, byte) in remaining.as_bytes()[open + 1..].iter().enumerate() {
            match byte {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                close = Some(open + 1 + offset);
                break;
            }
        }
        let close = close.ok_or("contains an unterminated action block")?;
        let action = &remaining[open + 1..close];
        remaining = &remaining[close + 1..];
        if ["struct ", "union ", "enum "]
            .iter()
            .any(|kind| header.starts_with(kind))
        {
            if !remaining.trim_start().starts_with(';') {
                return Err("contains an incomplete type declaration");
            }
            continue;
        }
        if action
            .split("@asb_tree")
            .skip(1)
            .any(|tail| !tail.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_'))
        {
            return Err(
                "references the reserved @asb_tree map in an action; use it only in probe predicates so custom code cannot change membership",
            );
        }
        if attachments(header) == Some(true) {
            continue;
        }
        // Probe paths can contain slashes (including unquoted uprobe paths).
        // A predicate ends at the final slash; test possible opening delimiters
        // using the positive boolean proof, not substring membership.
        let header = header.trim_end();
        let guarded = header.strip_suffix('/').is_some_and(|before_close| {
            before_close.match_indices('/').take(64).any(|(open, _)| {
                let attachment = &before_close[..open];
                attachments(attachment) == Some(false)
                    && readonly_predicate(&before_close[open + 1..])
                    && proof(&before_close[open + 1..], 0) == Proof::Scoped
            })
        });
        if !guarded {
            return Err(
                "contains an unscoped probe or unsupported predicate and is not process-tree scoped; every process-observing probe must require positive @asb_tree[pid], @asb_tree[tid], or relevant tracepoint PID membership (OR branches must each be scoped)",
            );
        }
        saw_probe = true;
    }
    if !saw_probe {
        return Err("has no process-tree-scoped probe");
    }
    Ok(())
}

/// Scope proof is valid only when evaluating its predicate cannot change the
/// membership map (including on an initially unrelated task).
fn readonly_predicate(expression: &str) -> bool {
    if [
        "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
    ]
    .iter()
    .any(|operator| expression.contains(operator))
    {
        return false;
    }
    let bytes = expression.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'='
            && bytes.get(index + 1) != Some(&b'=')
            && (index == 0 || !matches!(bytes[index - 1], b'=' | b'!' | b'<' | b'>'))
        {
            return false;
        }
        if !bytes[index].is_ascii_alphabetic() && bytes[index] != b'_' {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        let mut following = index;
        while following < bytes.len() && bytes[following].is_ascii_whitespace() {
            following += 1;
        }
        // str and strncmp are read-only. Unknown or mutating calls must move
        // inside an already-scoped action; they cannot participate in proof.
        if bytes.get(following) == Some(&b'(')
            && !matches!(&expression[start..index], "str" | "strncmp")
        {
            return false;
        }
    }
    true
}

/// Recognize attachment boundaries, not action syntax. In particular, a slash
/// within a binary path cannot be confused with a predicate delimiter.
fn attachments(header: &str) -> Option<bool> {
    let mut control_only = true;
    for part in header.split(',') {
        let part = part.trim();
        if matches!(part, "BEGIN" | "END") {
            continue;
        }
        let fields: Vec<_> = part.split(':').collect();
        if fields.len() < 2 || fields[0].is_empty() {
            return None;
        }
        let path_provider = matches!(fields[0], "uprobe" | "uretprobe" | "usdt");
        if path_provider && fields.len() < 3 {
            return None;
        }
        for (index, field) in fields.iter().enumerate() {
            if path_provider && index == 1 {
                continue;
            }
            let field = field.trim();
            if field.chars().any(|c| {
                c.is_whitespace()
                    || matches!(c, '/' | '@' | '&' | '|' | '=' | ';' | '(' | ')' | '{' | '}')
            }) {
                return None;
            }
        }
        control_only &= fields[0] == "interval";
    }
    Some(control_only)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Proof {
    Scoped,
    Never,
    Unknown,
}

fn proof(expression: &str, depth: usize) -> Proof {
    if depth > 64 {
        return Proof::Unknown;
    }
    let expression = expression.trim();
    let mut parens = 0_i32;
    let mut brackets = 0_i32;
    let mut ors = Vec::new();
    let mut ands = Vec::new();
    let mut outer_close = None;
    for (index, byte) in expression.bytes().enumerate() {
        match byte {
            b'(' => parens += 1,
            b')' => {
                parens -= 1;
                if parens == 0 && outer_close.is_none() {
                    outer_close = Some(index);
                }
            }
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            _ => {}
        }
        if parens < 0 || brackets < 0 {
            return Proof::Unknown;
        }
        if parens == 0 && brackets == 0 {
            if matches!(byte, b'?' | b',' | b';') {
                return Proof::Unknown;
            }
            let bytes = expression.as_bytes();
            if index == 0 || bytes[index - 1] != byte {
                if bytes[index..].starts_with(b"||") {
                    ors.push(index);
                }
                if bytes[index..].starts_with(b"&&") {
                    ands.push(index);
                }
            }
        }
    }
    if parens != 0 || brackets != 0 {
        return Proof::Unknown;
    }
    let positions = if !ors.is_empty() { &ors } else { &ands };
    if !positions.is_empty() {
        let mut values = Vec::new();
        let mut start = 0;
        for &position in positions {
            if expression[start..position].trim().is_empty() {
                return Proof::Unknown;
            }
            values.push(proof(&expression[start..position], depth + 1));
            start = position + 2;
        }
        if expression[start..].trim().is_empty() {
            return Proof::Unknown;
        }
        values.push(proof(&expression[start..], depth + 1));
        return if !ors.is_empty() {
            if values.contains(&Proof::Unknown) {
                Proof::Unknown
            } else if values.contains(&Proof::Scoped) {
                Proof::Scoped
            } else {
                Proof::Never
            }
        } else if values.contains(&Proof::Never) {
            Proof::Never
        } else if values.contains(&Proof::Scoped) {
            Proof::Scoped
        } else {
            Proof::Unknown
        };
    }
    if expression.starts_with('(') && outer_close == expression.len().checked_sub(1) {
        return proof(&expression[1..expression.len() - 1], depth + 1);
    }
    let atom: String = expression.chars().filter(|c| !c.is_whitespace()).collect();
    if matches!(atom.as_str(), "0" | "false") {
        return Proof::Never;
    }
    for key in [
        "pid",
        "tid",
        "args->pid",
        "args->prev_pid",
        "args->next_pid",
        "args->parent_pid",
        "args->child_pid",
    ] {
        let member = format!("@asb_tree[{key}]");
        if atom == member
            || ["!=0", ">0", ">=1", "==1"]
                .iter()
                .any(|suffix| atom == format!("{member}{suffix}"))
        {
            return Proof::Scoped;
        }
    }
    Proof::Unknown
}

#[cfg(test)]
mod tests;
