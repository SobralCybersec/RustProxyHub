use anyhow::{anyhow, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

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
    pub browser_available: bool,
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

/// Node binary name for the current platform. The bundle ships `node.exe` for
/// Windows; on Unix that PE binary can't execute, so we look for the system
/// `node` (bundled under a matching name, or on PATH).
pub fn node_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "node.exe"
    } else {
        "node"
    }
}

/// First `node` found on PATH — the Unix fallback, since the bundle only carries
/// a Windows binary. Resolved to an absolute path in the same process (and thus
/// the same PATH, incl. version-manager shims) that will later spawn it.
fn find_node_on_path() -> Option<PathBuf> {
    let name = node_binary_name();
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

pub fn resolve_node_candidates_for_tests(
    resource_dir: Option<&Path>,
    workspace_root: &Path,
) -> Vec<PathBuf> {
    let bin = node_binary_name();
    let mut candidates = Vec::new();
    if let Some(resource_dir) = resource_dir {
        candidates.push(resource_dir.join("node").join(bin));
        candidates.push(resource_dir.join("resources").join("node").join(bin));
    }
    candidates.push(
        workspace_root
            .join("src-tauri")
            .join("resources")
            .join("node")
            .join(bin),
    );
    candidates
}

pub fn resolve_runtime_paths(
    resource_dir: Option<&Path>,
    workspace_root: &Path,
) -> ResolvedRuntimePaths {
    let helper_dir = resolve_helper_dir_from(resource_dir, workspace_root);
    let node_candidates = resolve_node_candidates_for_tests(resource_dir, workspace_root);
    // Prefer a bundled node for the platform; fall back to system node on PATH.
    let node_path = node_candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .or_else(find_node_on_path);
    let node_source = node_path
        .as_ref()
        .map(|path| classify_node_source(path, resource_dir, workspace_root));

    ResolvedRuntimePaths {
        helper_dir,
        node_path,
        node_source,
    }
}

/// True when a Chromium-family browser Playwright can drive (Edge, Chrome, or
/// Chromium) is present. The bridge runs the `chromium` engine with an optional
/// `msedge`/`chrome` channel, so any of the three satisfies the runtime. Checks
/// known install paths on Windows, Linux, and macOS, then falls back to a PATH
/// lookup for non-standard installs (snaps, custom prefixes).
pub fn detect_browser_available() -> bool {
    // Absolute install locations across all three platforms.
    const CANDIDATES: &[&str] = &[
        // Windows — Edge
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files (x86)\Microsoft\Edge Beta\Application\msedge.exe",
        // Windows — Chrome
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        // Linux — Edge
        "/usr/bin/microsoft-edge",
        "/usr/bin/microsoft-edge-stable",
        "/usr/bin/microsoft-edge-beta",
        "/usr/bin/microsoft-edge-dev",
        "/opt/microsoft/msedge/msedge",
        // Linux — Chrome
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/opt/google/chrome/chrome",
        // Linux — Chromium
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
        // macOS
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];
    if CANDIDATES.iter().any(|path| Path::new(path).exists()) {
        return true;
    }

    // Windows per-user installs under LOCALAPPDATA.
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(local_app_data);
        let per_user = [
            base.join(r"Microsoft\Edge\Application\msedge.exe"),
            base.join(r"Google\Chrome\Application\chrome.exe"),
        ];
        if per_user.iter().any(|path| path.exists()) {
            return true;
        }
    }

    // PATH lookup — covers custom prefixes and aliased snaps on any platform.
    const BINARIES: &[&str] = &[
        "microsoft-edge",
        "microsoft-edge-stable",
        "microsoft-edge-beta",
        "microsoft-edge-dev",
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ];
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if BINARIES.iter().any(|bin| dir.join(bin).exists()) {
                return true;
            }
        }
    }

    false
}

pub fn build_runtime_diagnostics(
    resource_dir: Option<&Path>,
    workspace_root: &Path,
    app_data_dir: &Path,
    browser_available: bool,
) -> RuntimeDiagnostics {
    let resolved = resolve_runtime_paths(resource_dir, workspace_root);
    let mut issues = Vec::new();

    if resolved.helper_dir.is_none() {
        issues.push("Bundled playwright bridge folder not found.".to_owned());
    }
    if resolved.node_path.is_none() {
        issues.push(
            "Node.js runtime not found (no bundled node and none on PATH). Install Node.js."
                .to_owned(),
        );
    }
    if !browser_available {
        issues.push(
            "No supported browser found. Install Microsoft Edge, Google Chrome, or Chromium to run browser-backed providers.".to_owned(),
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
        browser_available,
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
        .ok_or_else(|| anyhow!("Node.js runtime unavailable"))
}

fn classify_node_source(path: &Path, resource_dir: Option<&Path>, workspace_root: &Path) -> String {
    let bin = node_binary_name();
    if let Some(resource_dir) = resource_dir {
        let direct = resource_dir.join("node").join(bin);
        let nested = resource_dir.join("resources").join("node").join(bin);
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
        .join(bin);
    if path == dev {
        "dev-resource".to_owned()
    } else {
        // Resolved from PATH (the Unix fallback) rather than a bundled location.
        "system-path".to_owned()
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
        build_runtime_diagnostics, node_binary_name, resolve_helper_dir_from,
        resolve_node_candidates_for_tests, resolve_runtime_paths,
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
                .join(node_binary_name()),
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
                    .join(node_binary_name())
            )
        );
        assert_eq!(resolved.node_source.as_deref(), Some("dev-resource"));
    }

    #[test]
    fn resolves_bundled_resource_layout_paths() {
        let root = temp_dir("bundle-layout");
        let resources = root.join("bundle");
        touch(&resources.join("playwright-bridge").join("index.mjs"));
        touch(&resources.join("node").join(node_binary_name()));

        let resolved = resolve_runtime_paths(Some(&resources), &root);
        assert_eq!(
            resolved.helper_dir,
            Some(resources.join("playwright-bridge"))
        );
        assert_eq!(
            resolved.node_path,
            Some(resources.join("node").join(node_binary_name()))
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
        touch(
            &resources
                .join("resources")
                .join("node")
                .join(node_binary_name()),
        );

        let resolved = resolve_runtime_paths(Some(&resources), &root);
        assert_eq!(
            resolved.helper_dir,
            Some(resources.join("resources").join("playwright-bridge"))
        );
        assert_eq!(
            resolved.node_path,
            Some(
                resources
                    .join("resources")
                    .join("node")
                    .join(node_binary_name())
            )
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
                resources.join("node").join(node_binary_name()),
                resources
                    .join("resources")
                    .join("node")
                    .join(node_binary_name()),
                root.join("src-tauri")
                    .join("resources")
                    .join("node")
                    .join(node_binary_name()),
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
        // Node is not asserted missing here: resolution now falls back to a
        // system `node` on PATH, which exists on any dev/CI machine.
        assert!(diagnostics
            .issues
            .iter()
            .any(|issue| issue.contains("supported browser")));
    }
}
