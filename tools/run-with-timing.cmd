@echo off
REM Launch a game with the x86 bridge stage probe enabled.
REM
REM   tools\run-with-timing.cmd "D:\Games\Some Game\game.exe"
REM
REM The variable is set inside this script's own environment and inherited by the game, so nothing
REM is left behind afterwards. That is the difference from setx, which would turn the probe on for
REM every process started from then on until you remember to clear it.
REM
REM The frontend reads the variable once at startup and reports probe=on in dlss5-neural-x86.log.
REM It then averages one line per 120 completed frames splitting the bridge into input+prepare,
REM host and output. Off by default; see docs/x86bridge.md for how to read the numbers.

setlocal

if "%~1"=="" (
    echo usage: run-with-timing.cmd "path\to\game.exe"
    exit /b 2
)
if not exist "%~1" (
    echo Game executable not found: %~1
    exit /b 1
)

set "DLSS5_X86BRIDGE_TIMING=1"
echo DLSS5_X86BRIDGE_TIMING=1
echo Launching "%~1"
start "" /D "%~dp1" "%~1"

endlocal
