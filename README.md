# RustProxyHub

RustProxyHub now ships as one Windows desktop runner. Users launch `tauri-app.exe` or the generated `*-setup.exe`; they do not launch provider executables because there are none.

## Supported Windows Flow

- Primary artifact: the NSIS `-setup.exe` installer.
- Secondary artifact: the portable `tauri-app.exe` bundle for advanced users.
- Both artifacts come from the same Tauri build path and bundle the internal helper bridge, `node.exe`, and Playwright runtime dependencies.

## Runtime Requirements

- Microsoft Edge must be installed. Browser-backed providers use the local Edge channel.
- WebView2 stays on the default bootstrapper flow. If WebView2 is missing, Windows will download it during install/runtime setup.
- App data must be writable because provider state and Qwen account storage live under the user app-data directory.

## Daily Commands

- `pnpm verify`
  Runs ESLint, TypeScript, Vitest, frontend build, Rust tests, and Rust clippy.
- `pnpm tauri build --debug`
  Smoke-checks debug packaging and bundled runtime resources.
- `pnpm release:windows`
  Runs `pnpm verify`, builds release artifacts, and confirms the portable exe plus bundled Node/helper resources are present.

## Troubleshooting

- Runtime status shows `degraded`
  Open the dashboard header and fix the listed preflight issue before trusting provider status.
- `node.exe missing from bundle`
  Rebuild with `pnpm tauri build` and confirm `src-tauri/resources/node/node.exe` exists.
- `Microsoft Edge not found`
  Install Edge, then relaunch the app.
- Qwen account actions fail
  Check that the app-data folder is writable and the dashboard runtime issues list is empty.
