@echo off
rem Opens the WEPANET desktop application (Windows).
rem With Rust installed (https://rustup.rs) it is built, or brought up to date, first;
rem the first build takes a few minutes. Without Rust the last build in desktop-app\ is used.
setlocal
set "ROOT=%~dp0"
set "EXE=%ROOT%desktop-app\WEPANET.exe"
rem Build output stays out of Dropbox/OneDrive, which lock files while syncing.
set "CARGO_TARGET_DIR=%LOCALAPPDATA%\cargo-target\wepanet-desktop"
where cargo >nul 2>nul
if errorlevel 1 goto run
echo Building WEPANET Desktop...
cargo build --release -p epanet-ui --manifest-path "%ROOT%Cargo.toml"
if errorlevel 1 goto failed
if not exist "%ROOT%desktop-app" mkdir "%ROOT%desktop-app"
copy /y "%CARGO_TARGET_DIR%\release\epanet-ui.exe" "%EXE%" >nul
:run
if not exist "%EXE%" (
    echo WEPANET Desktop has not been built yet. Install Rust from https://rustup.rs and run this file again.
    pause
    exit /b 1
)
start "" "%EXE%" %*
exit /b 0
:failed
echo The build failed; see the messages above.
pause
exit /b 1
