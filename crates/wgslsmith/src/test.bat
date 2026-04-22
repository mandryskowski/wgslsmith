#!cmd.exe /c
@echo off
setlocal EnableDelayedExpansion
set args=%WGSLREDUCE_KIND% %WGSLREDUCE_SHADER_NAME% %WGSLREDUCE_METADATA_PATH%
if defined WGSLREDUCE_SERVER ( set args=!args! --server %WGSLREDUCE_SERVER% )
if defined WGSLREDUCE_TARGETS ( set args=!args! %WGSLREDUCE_TARGETS% )
if "%WGSLREDUCE_KIND%"=="crash" (
    set args=!args! --regex "%WGSLREDUCE_REGEX%"
    if defined WGSLREDUCE_INVERSE_REGEX ( set args=!args! --inverse-regex "%WGSLREDUCE_INVERSE_REGEX%" )
    if defined WGSLREDUCE_CONFIG (
        set args=!args! --config %WGSLREDUCE_CONFIG%
    ) else (
        set args=!args! --compiler %WGSLREDUCE_COMPILER% --backend %WGSLREDUCE_BACKEND%
    )
    if not defined WGSLREDUCE_RECONDITION ( set args=!args! --no-recondition )
    if defined WGSLREDUCE_PRE_CMD ( set args=!args! --pre-cmd "%WGSLREDUCE_PRE_CMD%" )
    if defined WGSLREDUCE_POST_CMD ( set args=!args! --post-cmd "%WGSLREDUCE_POST_CMD%" )
)
"[WGSLSMITH]" test -q !args!
exit /b %errorlevel%
