import fs from "node:fs"
import path from "node:path"
import { createRequire } from "node:module"
import { fileURLToPath } from "node:url"

const require = createRequire(import.meta.url)
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const resourcesRoot = path.join(repoRoot, "src-tauri", "resources", "node_modules")

function resolvePackageDir(specifier, searchPaths) {
  const packageJsonPath = require.resolve(`${specifier}/package.json`, {
    paths: searchPaths,
  })
  return path.dirname(fs.realpathSync(packageJsonPath))
}

function copyPackage(specifier, sourceDir) {
  const targetDir = path.join(resourcesRoot, specifier)
  fs.rmSync(targetDir, { force: true, recursive: true })
  fs.cpSync(sourceDir, targetDir, { dereference: true, recursive: true })
  console.log(`Vendored ${specifier} -> ${targetDir}`)
}

fs.mkdirSync(resourcesRoot, { recursive: true })

const playwrightDir = resolvePackageDir("playwright", [repoRoot])
const playwrightCoreDir = resolvePackageDir("playwright-core", [playwrightDir, repoRoot])

copyPackage("playwright", playwrightDir)
copyPackage("playwright-core", playwrightCoreDir)
