@echo off
setlocal

set "WORKSPACE=%~dp0.."
for %%I in ("%WORKSPACE%") do set "WORKSPACE=%%~fI"
set "TARGET=%WORKSPACE%\target\debug"
set "RUNTIME=%WORKSPACE%\runtime"

if not exist "%RUNTIME%" mkdir "%RUNTIME%"

set "HOST=127.0.0.1"
set "API_KEY="

set "PORT=3000"
set "BROWSER=chromium"
set "HEADLESS=true"
start "qwen-smoke" /b cmd /c ""%TARGET%\qwen-proxy-rs.exe" server 1>"%RUNTIME%\qwen-smoke.out.log" 2>"%RUNTIME%\qwen-smoke.err.log""

set "PORT=3001"
start "deepseek-smoke" /b cmd /c ""%TARGET%\deepseek-proxy-rs.exe" server 1>"%RUNTIME%\deepseek-smoke.out.log" 2>"%RUNTIME%\deepseek-smoke.err.log""

set "PORT=3002"
start "kimi-smoke" /b cmd /c ""%TARGET%\kimi-proxy-rs.exe" server 1>"%RUNTIME%\kimi-smoke.out.log" 2>"%RUNTIME%\kimi-smoke.err.log""

set "PORT=3100"
set "QWEN_BASE_URL=http://127.0.0.1:3000"
set "DEEPSEEK_BASE_URL=http://127.0.0.1:3001"
set "KIMI_BASE_URL=http://127.0.0.1:3002"
start "hub-smoke" /b cmd /c ""%TARGET%\hub-proxy-rs.exe" server 1>"%RUNTIME%\hub-smoke.out.log" 2>"%RUNTIME%\hub-smoke.err.log""

timeout /t 5 /nobreak >nul
echo stack-started
