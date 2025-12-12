@echo off
echo Testing setVariable functionality in debugger

REM Test 1: Set initial variables
set NAME=Alice
set AGE=25
echo Initial values: NAME=%NAME%, AGE=%AGE%

REM Test 2: Modify variables during debugging
REM (User can modify these in VSCode variables panel)
echo Current NAME: %NAME%
echo Current AGE: %AGE%

REM Test 3: SETLOCAL scope test
call :test_local_scope
echo After subroutine, global NAME: %NAME%

REM Test 4: Special characters in values
set PATH_VAR=C:\Program Files\Test
set EQUALS_VAR=key=value
echo PATH_VAR=%PATH_VAR%
echo EQUALS_VAR=%EQUALS_VAR%

echo Test complete
exit /b 0

:test_local_scope
setlocal
set LOCAL_NAME=Bob
echo In subroutine: LOCAL_NAME=%LOCAL_NAME%
echo In subroutine: NAME=%NAME%
endlocal
exit /b 0
