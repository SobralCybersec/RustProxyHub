@echo off
taskkill /F /IM hub-proxy-rs.exe >nul 2>nul
taskkill /F /IM qwen-proxy-rs.exe >nul 2>nul
taskkill /F /IM deepseek-proxy-rs.exe >nul 2>nul
taskkill /F /IM kimi-proxy-rs.exe >nul 2>nul
echo stack-stopped
