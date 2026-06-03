#!cmd.exe /c
@echo off
setlocal EnableDelayedExpansion
set args=%WGSLREDUCE_KIND% %WGSLREDUCE_SHADER_NAME% %WGSLREDUCE_METADATA_PATH%
if defined WGSLREDUCE_SERVER ( set args=!args! --server %WGSLREDUCE_SERVER% )
if defined WGSLREDUCE_TARGETS ( set args=!args! %WGSLREDUCE_TARGETS% )
if defined WGSLREDUCE_USE_DAEMON (
    set args=!args! --use-daemon
    if defined WGSLREDUCE_DAEMON_PORT ( set args=!args! --daemon-port %WGSLREDUCE_DAEMON_PORT% )
)
if defined WGSLREDUCE_CONFIGS (
    for %%C in (%WGSLREDUCE_CONFIGS%) do set args=!args! --config %%C
)
if "%WGSLREDUCE_KIND%"=="crash" (
    set args=!args! --regex "%WGSLREDUCE_REGEX%"
    if defined WGSLREDUCE_INVERSE_REGEX ( set args=!args! --inverse-regex "%WGSLREDUCE_INVERSE_REGEX%" )
    if not defined WGSLREDUCE_CONFIGS (
        set args=!args! --compiler %WGSLREDUCE_COMPILER% --backend %WGSLREDUCE_BACKEND%
    )
    if not defined WGSLREDUCE_RECONDITION ( set args=!args! --no-recondition )
    if defined WGSLREDUCE_PRE_CMD ( set args=!args! --pre-cmd "%WGSLREDUCE_PRE_CMD%" )
    if defined WGSLREDUCE_POST_CMD ( set args=!args! --post-cmd "%WGSLREDUCE_POST_CMD%" )
)
if defined WGSLREDUCE_ATTEMPTS ( set args=!args! --attempts %WGSLREDUCE_ATTEMPTS% )
"[WGSLSMITH]" test -q !args!
exit /b %errorlevel%
