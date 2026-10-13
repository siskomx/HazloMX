@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 1
rtk proxy cargo test -p hazlomexa host_tools::tests::actual_mcp_cadpilot_measured_followup -- --ignored --exact --nocapture
exit /b %errorlevel%
