$workspace = Split-Path -Parent $PSScriptRoot
$target = Join-Path $workspace 'target\debug'
$runtime = Join-Path $workspace 'runtime'

New-Item -ItemType Directory -Force -Path $runtime | Out-Null

$definitions = @(
  @{
    Name = 'qwen'
    Port = '3000'
    Exe = 'qwen-proxy-rs.exe'
    Env = @{
      BROWSER = 'chromium'
      HEADLESS = 'true'
    }
  }
  @{
    Name = 'deepseek'
    Port = '3001'
    Exe = 'deepseek-proxy-rs.exe'
    Env = @{
      BROWSER = 'chromium'
      HEADLESS = 'true'
    }
  }
  @{
    Name = 'kimi'
    Port = '3002'
    Exe = 'kimi-proxy-rs.exe'
    Env = @{
      BROWSER = 'chromium'
      HEADLESS = 'true'
    }
  }
  @{
    Name = 'hub'
    Port = '3100'
    Exe = 'hub-proxy-rs.exe'
    Env = @{
      QWEN_BASE_URL = 'http://127.0.0.1:3000'
      DEEPSEEK_BASE_URL = 'http://127.0.0.1:3001'
      KIMI_BASE_URL = 'http://127.0.0.1:3002'
    }
  }
)

$started = @()
foreach ($definition in $definitions) {
  $env:HOST = '127.0.0.1'
  $env:PORT = $definition.Port
  Remove-Item Env:API_KEY -ErrorAction SilentlyContinue
  Remove-Item Env:BROWSER -ErrorAction SilentlyContinue
  Remove-Item Env:HEADLESS -ErrorAction SilentlyContinue
  Remove-Item Env:QWEN_BASE_URL -ErrorAction SilentlyContinue
  Remove-Item Env:DEEPSEEK_BASE_URL -ErrorAction SilentlyContinue
  Remove-Item Env:KIMI_BASE_URL -ErrorAction SilentlyContinue

  foreach ($item in $definition.Env.GetEnumerator()) {
    Set-Item -Path "Env:$($item.Key)" -Value $item.Value
  }

  $stdout = Join-Path $runtime "$($definition.Name)-smoke.out.log"
  $stderr = Join-Path $runtime "$($definition.Name)-smoke.err.log"
  $process = Start-Process `
    -FilePath (Join-Path $target $definition.Exe) `
    -ArgumentList 'server' `
    -WorkingDirectory $workspace `
    -WindowStyle Hidden `
    -RedirectStandardOutput $stdout `
    -RedirectStandardError $stderr `
    -PassThru

  $started += [pscustomobject]@{
    name = $definition.Name
    pid = $process.Id
    stdout = $stdout
    stderr = $stderr
  }
}

Start-Sleep -Seconds 5
$started | ConvertTo-Json -Compress
