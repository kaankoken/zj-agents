use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use zj_agents_core::manifest::{bundled_manifests, CompiledManifest, ManifestError};

pub const READ_MANIFESTS_SH: &str = r#"
set -eu
export LC_ALL=C
[ -d "$ZJA_DIR" ] || exit 0
for path in "$ZJA_DIR"/*.toml; do
    [ -f "$path" ] || continue
    name=${path##*/}
    name_len=${#name}
    content_len=$(wc -c < "$path")
    content_len=$(printf '%s' "$content_len" | tr -d '[:space:]')
    printf '%s\0%s\0%s' "$name_len" "$content_len" "$name"
    cat "$path"
done
"#;

#[derive(Clone, Debug, Default)]
pub struct ReloadController {
    active: bool,
    pending: bool,
}

impl ReloadController {
    pub fn request_start(&mut self) -> bool {
        if self.active {
            self.pending = true;
            false
        } else {
            self.active = true;
            true
        }
    }

    pub fn complete(&mut self) -> bool {
        self.active = false;
        if self.pending {
            self.pending = false;
            self.active = true;
            true
        } else {
            false
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrameError {
    InvalidLength,
    Truncated,
    TrailingJunk,
    NonUtf8Name,
    DuplicateName,
}

pub fn parse_manifest_frame(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, FrameError> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut seen = BTreeMap::new();
    while i < bytes.len() {
        let (name_len, ni) = read_decimal_nul(bytes, i)?;
        let (content_len, ci) = read_decimal_nul(bytes, ni)?;
        if ci + name_len + content_len > bytes.len() {
            return Err(FrameError::Truncated);
        }
        let name_bytes = &bytes[ci..ci + name_len];
        let content = bytes[ci + name_len..ci + name_len + content_len].to_vec();
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| FrameError::NonUtf8Name)?
            .to_owned();
        if seen.insert(name.clone(), ()).is_some() {
            return Err(FrameError::DuplicateName);
        }
        out.push((name, content));
        i = ci + name_len + content_len;
    }
    Ok(out)
}

fn read_decimal_nul(bytes: &[u8], start: usize) -> Result<(usize, usize), FrameError> {
    let mut i = start;
    if i >= bytes.len() {
        return Err(FrameError::Truncated);
    }
    let mut value: usize = 0;
    let mut digits = 0;
    while i < bytes.len() && bytes[i] != 0 {
        let b = bytes[i];
        if !b.is_ascii_digit() {
            return Err(FrameError::InvalidLength);
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
            .ok_or(FrameError::InvalidLength)?;
        digits += 1;
        i += 1;
    }
    if digits == 0 || i >= bytes.len() || bytes[i] != 0 {
        return Err(FrameError::InvalidLength);
    }
    Ok((value, i + 1))
}

#[derive(Debug)]
pub struct ManifestDirError;

pub fn resolve_manifest_dir(
    configured: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, ManifestDirError> {
    let path = match configured {
        None | Some("") => {
            let home = home.ok_or(ManifestDirError)?;
            PathBuf::from(home).join(".config/zellij/zj-agents/agent-detection")
        }
        Some(raw) if raw.starts_with("~/") => {
            let home = home.ok_or(ManifestDirError)?;
            PathBuf::from(home).join(&raw[2..])
        }
        Some(raw) if Path::new(raw).is_absolute() => PathBuf::from(raw),
        Some(_) => return Err(ManifestDirError),
    };
    Ok(path)
}

pub fn adopt_overrides(
    records: &[(String, Vec<u8>)],
) -> Result<Vec<CompiledManifest>, ManifestError> {
    let mut set = bundled_manifests().map_err(|_| ManifestError::Parse)?;
    for (filename, content) in records {
        let text = std::str::from_utf8(content).map_err(|_| ManifestError::Parse)?;
        let compiled = CompiledManifest::parse(filename, text)?;
        if let Some(idx) = set.iter().position(|m| m.name() == compiled.name()) {
            set[idx] = compiled;
        } else {
            set.push(compiled);
        }
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zero_and_multiple_records() {
        assert!(parse_manifest_frame(b"").unwrap().is_empty());
        let body = b"hello = 1\n";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"10\0");
        bytes.extend_from_slice(format!("{}\0", body.len()).as_bytes());
        bytes.extend_from_slice(b"hello.toml");
        bytes.extend_from_slice(body);
        let records = parse_manifest_frame(&bytes).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "hello.toml");
        assert_eq!(records[0].1, body);
    }

    #[test]
    fn embedded_newlines_and_markers_ok() {
        let content = b"x = \"a\\n---\\n\"";
        let name = b"a.toml";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(format!("{}\0", name.len()).as_bytes());
        bytes.extend_from_slice(format!("{}\0", content.len()).as_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(content);
        let records = parse_manifest_frame(&bytes).unwrap();
        assert_eq!(records[0].1, content);
    }

    #[test]
    fn truncation_fails() {
        assert_eq!(parse_manifest_frame(b"5\0"), Err(FrameError::Truncated));
    }

    #[test]
    fn reload_controller_pending_followup() {
        let mut c = ReloadController::default();
        assert!(c.request_start());
        assert!(!c.request_start());
        assert!(c.complete());
        assert!(!c.complete());
    }

    #[test]
    fn resolve_dir_rules() {
        let home = Some("/home/u");
        assert_eq!(
            resolve_manifest_dir(None, home).unwrap(),
            PathBuf::from("/home/u/.config/zellij/zj-agents/agent-detection")
        );
        assert_eq!(
            resolve_manifest_dir(Some("~/x"), home).unwrap(),
            PathBuf::from("/home/u/x")
        );
        assert_eq!(
            resolve_manifest_dir(Some("/abs"), home).unwrap(),
            PathBuf::from("/abs")
        );
        assert!(resolve_manifest_dir(Some("rel"), home).is_err());
    }
}
