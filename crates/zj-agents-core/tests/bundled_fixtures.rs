use std::path::{Path, PathBuf};

use zj_agents_core::manifest::{bundled_manifests, classify};
use zj_agents_core::model::Observation;

const AGENTS: [&str; 5] = ["claude", "codex", "grok", "pi", "omp"];
const STATES: [&str; 3] = ["idle", "working", "blocked"];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_dir(agent: &str, state: &str) -> PathBuf {
    fixture_root().join(agent).join(state)
}

fn fixtures(agent: &str, state: &str) -> Vec<PathBuf> {
    let dir = fixture_dir(agent, state);
    let mut paths = std::fs::read_dir(&dir)
        .unwrap_or_else(|_| panic!("missing real fixture directory: {}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("txt"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn expected_observation(state: &str) -> Observation {
    match state {
        "idle" => Observation::Idle,
        "working" => Observation::Working,
        "blocked" => Observation::Blocked,
        other => panic!("unknown state {other}"),
    }
}

#[derive(serde::Deserialize)]
struct FixtureMetadata {
    agent_version: String,
    capture_date: String,
}

#[test]
fn every_bundled_agent_has_each_required_fixture_class() {
    for agent in AGENTS {
        for state in STATES {
            let dir = fixture_dir(agent, state);
            let count = std::fs::read_dir(&dir)
                .unwrap_or_else(|_| panic!("missing real fixture directory: {}", dir.display()))
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().and_then(|x| x.to_str()) == Some("txt"))
                .count();
            assert!(count > 0, "no reviewed {state} fixture for {agent}");
        }
    }
}

#[test]
fn every_real_fixture_records_version_and_capture_date() {
    for agent in AGENTS {
        for state in STATES {
            for fixture in fixtures(agent, state) {
                let metadata_path = fixture.with_extension("meta.toml");
                let metadata: FixtureMetadata = toml::from_str(
                    &std::fs::read_to_string(&metadata_path).unwrap_or_else(|_| {
                        panic!("missing fixture metadata: {}", metadata_path.display())
                    }),
                )
                .unwrap_or_else(|_| {
                    panic!("invalid fixture metadata: {}", metadata_path.display())
                });

                assert!(!metadata.agent_version.trim().is_empty());
                let date = metadata.capture_date.as_bytes();
                assert!(
                    date.len() == 10
                        && date[4] == b'-'
                        && date[7] == b'-'
                        && date
                            .iter()
                            .enumerate()
                            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit()),
                    "capture_date must be YYYY-MM-DD in {}",
                    metadata_path.display()
                );
            }
        }
    }
}

#[test]
fn bundled_manifests_classify_all_real_fixtures() {
    let manifests = bundled_manifests().unwrap();
    for agent in AGENTS {
        let manifest = manifests.iter().find(|m| m.name() == agent).unwrap();
        for state in STATES {
            for fixture in fixtures(agent, state) {
                let viewport = std::fs::read_to_string(&fixture)
                    .unwrap()
                    .lines()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                assert_eq!(
                    classify(manifest, &viewport).observation,
                    expected_observation(state),
                    "{}",
                    fixture.display()
                );
            }
        }
    }
}
