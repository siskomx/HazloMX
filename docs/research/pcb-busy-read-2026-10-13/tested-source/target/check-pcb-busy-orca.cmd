@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 1
rtk proxy cargo test -p hazlomexa --lib orca_run::tests::actual_orca_captures_profiles_and_slices_verified_files -- --ignored --exact --nocapture
exit /b %errorlevel%
