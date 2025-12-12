@echo off
echo Testing expression evaluator in debugger

REM Test 1: Simple variables
set NAME=Alice
set AGE=25
set CITY=NewYork
echo Variables set: NAME=%NAME%, AGE=%AGE%, CITY=%CITY%

REM Test 2: Complex expressions
REM (User can evaluate these in debug console)
REM Try: %NAME%
REM Try: NAME
REM Try: %AGE%
REM Try: %NAME% is %AGE% years old
REM Try: ERRORLEVEL

REM Test 3: Path expressions
set BASE_DIR=C:\Users
set SUB_DIR=Documents
echo Base: %BASE_DIR%
echo Try evaluating: %BASE_DIR%\%SUB_DIR%

REM Test 4: Arithmetic (will be evaluated by CMD)
REM Try evaluating in console: 2+2
REM Try: %NAME% lives in %CITY%

REM Test 5: SETLOCAL scope
call :test_local_scope
echo Back in main, NAME=%NAME%

REM Test 6: ERRORLEVEL testing
echo Success command
REM ERRORLEVEL should be 0, try evaluating: ERRORLEVEL

findstr "NOTFOUND" nonexistent.txt 2>nul
REM ERRORLEVEL should be non-zero now, try evaluating: ERRORLEVEL

echo Test complete
exit /b 0

:test_local_scope
setlocal
set LOCAL_VAR=LocalValue
set NAME=Bob
echo In subroutine: LOCAL_VAR=%LOCAL_VAR%, NAME=%NAME%
REM Try evaluating: LOCAL_VAR
REM Try evaluating: NAME
endlocal
exit /b 0
