@echo off
rem Publish a Vinteria build to the public GitHub update repo (the Update button in the header pulls it from there).
rem One-time: run   tools\publish_github.cmd https://github.com/YOUR_NAME/vinteria-updates.git
rem The raw base URL (https://raw.githubusercontent.com/YOUR_NAME/vinteria-updates/main) goes into update_url.txt next to the program.
setlocal
set "ROOT=%~dp0.."
pushd "%ROOT%"
call cargo build --release || exit /b 1
if not exist updates mkdir updates
copy /y target\release\VinteriaDaw.exe updates\VinteriaDaw.exe >nul
rem build id = the exe's modified time in unix seconds (the program compares it with its own exe)
powershell -NoProfile -Command "$t=[DateTimeOffset](Get-Item 'updates\VinteriaDaw.exe').LastWriteTimeUtc; $n=$t.ToUnixTimeSeconds(); Set-Content -Encoding ascii updates\latest.json ('{\"build\": '+$n+'}')"
pushd updates
if not exist .git (
  git init -b main
  if "%~1"=="" (echo Pass the repo URL on the first run & exit /b 1)
  git remote add origin %~1
)
git add -A
git commit -m "Vinteria build" || echo nothing new
git push -u origin main || exit /b 1
popd
popd
echo Published. The Update button will offer it.
