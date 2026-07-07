use anyhow::{anyhow, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const EDGE_CANDIDATES: [&str; 4] = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Users\Default\AppData\Local\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files (x86)\Microsoft\Edge Beta\Application\msedge.exe",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct RuntimeDiagnostics {
    // ponytail: path strings redacted from the webview payload (H6 info-disclosure).
    // Fields stay usable internally and in tests; only IPC serialization skips them.
    #[serde(skip)]
    pub node_path: Option<String>,
    #[serde(skip)]
    pub node_source: Option<String>,
    #[serde(skip)]
    pub helper_dir: Option<String>,
    pub edge_available: bool,
    pub single_runner_ready: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRuntimePaths {
    pub helper_dir: Option<PathBuf>,
    pub node_path: Option<PathBuf>,
    pub node_source: Option<String>,
}

pub fn resolve_helper_dir_from(
    resource_dir: Option<&Path>,
    workspace_root: &Path,
) -> Option<PathBuf> {
    if let Some(resource_dir) = resource_dir {
        let direct = resource_dir.join("playwright-bridge");
        if direct.exists() {
            return Some(direct);
        }

        let nested = resource_dir.join("resources").join("playwright-bridge");
        if nested.exists() {
            return Some(nested);
        }
    }

    let dev = workspace_root
        .join("src-tauri")
        .join("resources")
        .join("playwright-bridge");
    dev.exists().then_some(dev)
}

pub fn resolve_node_candidates_for_tests(
    resource_dir: Option<&Path>,
    workspace_root: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(resource_dir) = resource_dir {
        candidates.push(resource_dir.join("node").join("node.exe"));
        candidates.push(resource_dir.join("resources").join("node").join("node.exe"));
    }
    candidates.push(
        workspace_root
            .join("src-tauri")
            .join("resources")
            .join("node")
            .join("node.exe"),
    );
    candidates
}

pub fn resolve_runtime_paths(
    resource_dir: Option<&Path>,
    workspace_root: &Path,
) -> ResolvedRuntimePaths {
    let helper_dir = resolve_helper_dir_from(resource_dir, workspace_root);
    let node_candidates = resolve_node_candidates_for_tests(resource_dir, workspace_root);
    let node_path = node_candidates.iter().find(|path| path.exists()).cloned();
    let node_source = node_path
        .as_ref()
        .map(|path| classify_node_source(path, resource_dir, workspace_root));

    ResolvedRuntimePaths {
        helper_dir,
        node_path,
        node_source,
    }
}

pub fn detect_edge_available() -> bool {
    if EDGE_CANDIDATES.iter().any(|path| Path::new(path).exists()) {
        return true;
    }

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let local_candidate = PathBuf::from(local_app_data)
            .join("Microsoft")
            .join("Edge")
            .join("Application")
            .join("msedge.exe");
        if local_candidate.exists() {
            return true;
        }
    }

    false
}

pub fn build_runtime_diagnostics(
    resource_dir: Option<&Path>,
    workspace_root: &Path,
    app_data_dir: &Path,
    edge_available: bool,
) -> RuntimeDiagnostics {
    let resolved = resolve_runtime_paths(resource_dir, workspace_root);
    let mut issues = Vec::new();

    if resolved.helper_dir.is_none() {
        issues.push("Bundled playwright bridge folder not found.".to_owned());
    }
    if resolved.node_path.is_none() {
        issues.push("Bundled node.exe not found in Tauri resources.".to_owned());
    }
    if !edge_available {
        issues.push(
            "Microsoft Edge not found. Install Edge to run browser-backed providers.".to_owned(),
        );
    }
    if let Err(err) = ensure_writable_dir(app_data_dir) {
        issues.push(format!(
            "App data directory is not writable: {} ({err})",
            app_data_dir.display()
        ));
    }

    RuntimeDiagnostics {
        node_path: resolved
            .node_path
            .as_ref()
            .map(|path| path.display().to_string()),
        node_source: resolved.node_source,
        helper_dir: resolved
            .helper_dir
            .as_ref()
            .map(|path| path.display().to_string()),
        edge_available,
        single_runner_ready: issues.is_empty(),
        issues,
    }
}

pub fn require_helper_dir(diagnostics: &RuntimeDiagnostics) -> Result<PathBuf> {
    diagnostics
        .helper_dir
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("playwright bridge helper directory unavailable"))
}

pub fn require_node_path(diagnostics: &RuntimeDiagnostics) -> Result<PathBuf> {
    diagnostics
        .node_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("bundled node.exe unavailable"))
}

fn classify_node_source(path: &Path, resource_dir: Option<&Path>, workspace_root: &Path) -> String {
    if let Some(resource_dir) = resource_dir {
        let direct = resource_dir.join("node").join("node.exe");
        let nested = resource_dir.join("resources").join("node").join("node.exe");
        if path == direct {
            return "bundled-resource".to_owned();
        }
        if path == nested {
            return "portable-bundle".to_owned();
        }
    }

    let dev = workspace_root
        .join("src-tauri")
        .join("resources")
        .join("node")
        .join("node.exe");
    if path == dev {
        "dev-resource".to_owned()
    } else {
        "unknown".to_owned()
    }
}

fn ensure_writable_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let probe = dir.join(".rustproxyhub-write-check");
    fs::write(&probe, b"ok")?;
    fs::remove_file(probe)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_runtime_diagnostics, resolve_helper_dir_from, resolve_node_candidates_for_tests,
        resolve_runtime_paths,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rustproxyhub-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"ok").unwrap();
    }

    #[test]
    fn resolves_dev_layout_paths() {
        let root = temp_dir("dev-layout");
        touch(
            &root
                .join("src-tauri")
                .join("resources")
                .join("playwright-bridge")
                .join("index.mjs"),
        );
        touch(
            &root
                .join("src-tauri")
                .join("resources")
                .join("node")
                .join("node.exe"),
        );

        let resolved = resolve_runtime_paths(None, &root);
        assert_eq!(
            resolved.helper_dir,
            Some(
                root.join("src-tauri")
                    .join("resources")
                    .join("playwright-bridge")
            )
        );
        assert_eq!(
            resolved.node_path,
            Some(
                root.join("src-tauri")
                    .join("resources")
                    .join("node")
                    .join("node.exe")
            )
        );
        assert_eq!(resolved.node_source.as_deref(), Some("dev-resource"));
    }

    #[test]
    fn resolves_bundled_resource_layout_paths() {
        let root = temp_dir("bundle-layout");
        let resources = root.join("bundle");
        touch(&resources.join("playwright-bridge").join("index.mjs"));
        touch(&resources.join("node").join("node.exe"));

        let resolved = resolve_runtime_paths(Some(&resources), &root);
        assert_eq!(
            resolved.helper_dir,
            Some(resources.join("playwright-bridge"))
        );
        assert_eq!(
            resolved.node_path,
            Some(resources.join("node").join("node.exe"))
        );
        assert_eq!(resolved.node_source.as_deref(), Some("bundled-resource"));
    }

    #[test]
    fn resolves_portable_bundle_layout_paths() {
        let root = temp_dir("portable-layout");
        let resources = root.join("portable");
        touch(
            &resources
                .join("resources")
                .join("playwright-bridge")
                .join("index.mjs"),
        );
        touch(&resources.join("resources").join("node").join("node.exe"));

        let resolved = resolve_runtime_paths(Some(&resources), &root);
        assert_eq!(
            resolved.helper_dir,
            Some(resources.join("resources").join("playwright-bridge"))
        );
        assert_eq!(
            resolved.node_path,
            Some(resources.join("resources").join("node").join("node.exe"))
        );
        assert_eq!(resolved.node_source.as_deref(), Some("portable-bundle"));
    }

    #[test]
    fn helper_dir_resolution_prefers_bundled_path() {
        let root = temp_dir("helper-path");
        let resources = root.join("bundle");
        touch(&resources.join("playwright-bridge").join("index.mjs"));
        touch(
            &root
                .join("src-tauri")
                .join("resources")
                .join("playwright-bridge")
                .join("index.mjs"),
        );

        assert_eq!(
            resolve_helper_dir_from(Some(&resources), &root),
            Some(resources.join("playwright-bridge"))
        );
    }

    #[test]
    fn node_candidates_prioritize_resource_before_dev() {
        let root = PathBuf::from("G:/repo");
        let resources = PathBuf::from("G:/bundle/resources");
        let candidates = resolve_node_candidates_for_tests(Some(&resources), &root);
        assert_eq!(
            candidates,
            vec![
                resources.join("node").join("node.exe"),
                resources.join("resources").join("node").join("node.exe"),
                root.join("src-tauri")
                    .join("resources")
                    .join("node")
                    .join("node.exe"),
            ]
        );
    }

    #[test]
    fn diagnostics_report_missing_dependencies() {
        let root = temp_dir("diag-root");
        let app_data = root.join("app-data");
        let diagnostics = build_runtime_diagnostics(None, &root, &app_data, false);

        assert!(!diagnostics.single_runner_ready);
        assert!(diagnostics
            .issues
            .iter()
            .any(|issue| issue.contains("playwright bridge")));
        assert!(diagnostics
            .issues
            .iter()
            .any(|issue| issue.contains("node.exe")));
        assert!(diagnostics
            .issues
            .iter()
            .any(|issue| issue.contains("Microsoft Edge")));
    }
}
