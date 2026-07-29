use std::path::Path;

use regex::Regex;
use serde::Deserialize;

use crate::model::Observation;

#[derive(Clone, Debug)]
pub struct CompiledManifest {
    name: String,
    label: String,
    scan_lines: usize,
    fallback: Option<Observation>,
    process: Vec<String>,
    argv_any: Vec<String>,
    rules: Vec<CompiledRule>,
}

#[derive(Clone, Debug)]
struct CompiledRule {
    state: Observation,
    pattern: Regex,
}

#[derive(Debug)]
pub enum ManifestError {
    Parse,
    Schema,
    Name,
    Label,
    Process,
    ScanLines,
    Fallback,
    Rules,
    Regex,
    FilenameMismatch,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let category = match self {
            Self::Parse => "parse",
            Self::Schema => "schema",
            Self::Name => "name",
            Self::Label => "label",
            Self::Process => "process",
            Self::ScanLines => "scan_lines",
            Self::Fallback => "fallback",
            Self::Rules => "rules",
            Self::Regex => "regex",
            Self::FilenameMismatch => "filename",
        };
        write!(f, "manifest error: {category}")
    }
}

impl std::error::Error for ManifestError {}

#[derive(Debug)]
pub struct ManifestSetError {
    pub message: String,
}

impl std::fmt::Display for ManifestSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ManifestSetError {}

#[derive(Deserialize)]
struct RawManifest {
    schema: u8,
    name: String,
    label: String,
    #[serde(default = "default_scan_lines")]
    scan_lines: usize,
    fallback: Option<String>,
    detect: RawDetect,
    #[serde(default)]
    rule: Vec<RawRule>,
}

fn default_scan_lines() -> usize {
    40
}

#[derive(Deserialize)]
struct RawDetect {
    process: Vec<String>,
    #[serde(default)]
    argv_any: Vec<String>,
}

#[derive(Deserialize)]
struct RawRule {
    state: String,
    pattern: String,
}

impl CompiledManifest {
    pub fn parse(filename: &str, text: &str) -> Result<Self, ManifestError> {
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or(ManifestError::FilenameMismatch)?;
        let raw: RawManifest = toml::from_str(text).map_err(|_| ManifestError::Parse)?;
        if raw.schema != 1 {
            return Err(ManifestError::Schema);
        }
        if !valid_name(&raw.name) {
            return Err(ManifestError::Name);
        }
        if raw.name != stem {
            return Err(ManifestError::FilenameMismatch);
        }
        if raw.label.trim().is_empty() {
            return Err(ManifestError::Label);
        }
        if raw.detect.process.is_empty() || raw.detect.process.iter().any(|p| p.trim().is_empty()) {
            return Err(ManifestError::Process);
        }
        if !(1..=500).contains(&raw.scan_lines) {
            return Err(ManifestError::ScanLines);
        }
        let fallback = match raw.fallback.as_deref() {
            None => None,
            Some("idle") => Some(Observation::Idle),
            Some(_) => return Err(ManifestError::Fallback),
        };
        if raw.rule.is_empty() {
            return Err(ManifestError::Rules);
        }
        let mut rules = Vec::with_capacity(raw.rule.len());
        for rule in raw.rule {
            let state = match rule.state.as_str() {
                "idle" => Observation::Idle,
                "working" => Observation::Working,
                "blocked" => Observation::Blocked,
                _ => return Err(ManifestError::Rules),
            };
            let pattern = Regex::new(&rule.pattern).map_err(|_| ManifestError::Regex)?;
            rules.push(CompiledRule { state, pattern });
        }
        Ok(Self {
            name: raw.name,
            label: raw.label,
            scan_lines: raw.scan_lines,
            fallback,
            process: raw.detect.process,
            argv_any: raw.detect.argv_any,
            rules,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn scan_lines(&self) -> usize {
        self.scan_lines
    }

    pub fn matches_argv(&self, argv: &[String]) -> bool {
        if argv.is_empty() {
            return false;
        }
        let basename = Path::new(&argv[0])
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(argv[0].as_str());
        if !self.process.iter().any(|p| p == basename) {
            return false;
        }
        if self.argv_any.is_empty() {
            return true;
        }
        let joined = argv.join(" ");
        self.argv_any.iter().any(|needle| joined.contains(needle))
    }
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

pub enum Detection<'a> {
    None,
    One(&'a CompiledManifest),
    Ambiguous(Vec<&'a str>),
}

pub fn detect<'a>(manifests: &'a [CompiledManifest], argv: &[String]) -> Detection<'a> {
    let mut matches: Vec<&CompiledManifest> =
        manifests.iter().filter(|m| m.matches_argv(argv)).collect();
    match matches.len() {
        0 => Detection::None,
        1 => Detection::One(matches.remove(0)),
        _ => {
            let mut names: Vec<&str> = matches.iter().map(|m| m.name()).collect();
            names.sort_unstable();
            Detection::Ambiguous(names)
        }
    }
}

pub struct Classification {
    pub observation: Observation,
    pub fallback_used: bool,
}

pub fn classify(manifest: &CompiledManifest, viewport: &[String]) -> Classification {
    let start = viewport.len().saturating_sub(manifest.scan_lines());
    let text = viewport[start..].join("\n");
    for rule in &manifest.rules {
        if rule.pattern.is_match(&text) {
            return Classification {
                observation: rule.state,
                fallback_used: false,
            };
        }
    }
    if let Some(fallback) = manifest.fallback {
        Classification {
            observation: fallback,
            fallback_used: true,
        }
    } else {
        Classification {
            observation: Observation::Unknown,
            fallback_used: false,
        }
    }
}

pub fn bundled_manifests() -> Result<Vec<CompiledManifest>, ManifestSetError> {
    [
        ("claude.toml", include_str!("../manifests/claude.toml")),
        ("codex.toml", include_str!("../manifests/codex.toml")),
        ("grok.toml", include_str!("../manifests/grok.toml")),
        ("pi.toml", include_str!("../manifests/pi.toml")),
        ("omp.toml", include_str!("../manifests/omp.toml")),
        ("agy.toml", include_str!("../manifests/agy.toml")),
    ]
    .into_iter()
    .map(|(name, text)| {
        CompiledManifest::parse(name, text).map_err(|e| ManifestSetError {
            message: format!("bundled {name}: {e}"),
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
schema = 1
name = "claude"
label = "Claude Code"
scan_lines = 40
fallback = "idle"

[detect]
process = ["claude"]
argv_any = []

[[rule]]
state = "blocked"
pattern = "(?i)permission"
"#;

    #[test]
    fn compiles_valid_manifest() {
        let manifest = CompiledManifest::parse("claude.toml", VALID).unwrap();
        assert_eq!(manifest.name(), "claude");
        assert_eq!(manifest.scan_lines(), 40);
    }

    #[test]
    fn rejects_schema() {
        let text = VALID.replace("schema = 1", "schema = 2");
        assert!(matches!(
            CompiledManifest::parse("claude.toml", &text),
            Err(ManifestError::Schema)
        ));
    }

    #[test]
    fn rejects_filename_mismatch() {
        assert!(matches!(
            CompiledManifest::parse("codex.toml", VALID),
            Err(ManifestError::FilenameMismatch)
        ));
    }

    #[test]
    fn rejects_empty_process() {
        let text = VALID.replace(r#"process = ["claude"]"#, r#"process = []"#);
        assert!(matches!(
            CompiledManifest::parse("claude.toml", &text),
            Err(ManifestError::Process)
        ));
    }

    #[test]
    fn rejects_scan_lines_out_of_range() {
        let text = VALID.replace("scan_lines = 40", "scan_lines = 0");
        assert!(matches!(
            CompiledManifest::parse("claude.toml", &text),
            Err(ManifestError::ScanLines)
        ));
    }

    #[test]
    fn rejects_non_idle_fallback() {
        let text = VALID.replace(r#"fallback = "idle""#, r#"fallback = "working""#);
        assert!(matches!(
            CompiledManifest::parse("claude.toml", &text),
            Err(ManifestError::Fallback)
        ));
    }

    #[test]
    fn rejects_invalid_rule_state() {
        let text = VALID.replace(r#"state = "blocked""#, r#"state = "done""#);
        assert!(matches!(
            CompiledManifest::parse("claude.toml", &text),
            Err(ManifestError::Rules)
        ));
    }

    #[test]
    fn rejects_invalid_regex() {
        let text = VALID.replace(r#"pattern = "(?i)permission""#, r#"pattern = "(""#);
        assert!(matches!(
            CompiledManifest::parse("claude.toml", &text),
            Err(ManifestError::Regex)
        ));
    }

    #[test]
    fn rejects_no_rules() {
        let text = r#"
schema = 1
name = "claude"
label = "Claude Code"
scan_lines = 40
[detect]
process = ["claude"]
"#;
        assert!(matches!(
            CompiledManifest::parse("claude.toml", text),
            Err(ManifestError::Rules)
        ));
    }

    #[test]
    fn detection_ambiguous_when_two_match() {
        let a = CompiledManifest::parse(
            "a.toml",
            r#"
schema = 1
name = "a"
label = "A"
[detect]
process = ["tool"]
[[rule]]
state = "idle"
pattern = "x"
"#,
        )
        .unwrap();
        let b = CompiledManifest::parse(
            "b.toml",
            r#"
schema = 1
name = "b"
label = "B"
[detect]
process = ["tool"]
[[rule]]
state = "idle"
pattern = "x"
"#,
        )
        .unwrap();
        let manifests = vec![a, b];
        match detect(&manifests, &["tool".into()]) {
            Detection::Ambiguous(names) => assert_eq!(names, vec!["a", "b"]),
            Detection::None | Detection::One(_) => panic!("expected ambiguous"),
        }
    }

    #[test]
    fn classify_first_rule_wins() {
        let m = CompiledManifest::parse("claude.toml", VALID).unwrap();
        let c = classify(&m, &["need permission please".into()]);
        assert_eq!(c.observation, Observation::Blocked);
        assert!(!c.fallback_used);
    }

    #[test]
    fn classify_uses_fallback() {
        let m = CompiledManifest::parse("claude.toml", VALID).unwrap();
        let c = classify(&m, &["nothing special".into()]);
        assert_eq!(c.observation, Observation::Idle);
        assert!(c.fallback_used);
    }

    #[test]
    fn argv_any_filters() {
        let m = CompiledManifest::parse(
            "x.toml",
            r#"
schema = 1
name = "x"
label = "X"
[detect]
process = ["node"]
argv_any = ["codex"]
[[rule]]
state = "idle"
pattern = "z"
"#,
        )
        .unwrap();
        assert!(!m.matches_argv(&["node".into(), "other".into()]));
        assert!(m.matches_argv(&["node".into(), "codex".into()]));
    }
}
