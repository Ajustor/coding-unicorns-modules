@echo off
REM Build the project
setlocal EnableDelayedExpansion

set "ROOT=%~dp0"
set /a COUNT=0

if not exist "%ROOT%target" (
    mkdir "%ROOT%target"
)

for %%f in (*.rs) do (
    set /a COUNT+=1
    echo Found %%f
)

:: Run cargo
call cargo build --release || goto :error
echo Built !COUNT! files > "%ROOT%build.log"
goto :eof

:error
echo Build failed with %ERRORLEVEL%
exit /b 1
