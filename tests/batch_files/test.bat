@echo off
echo Testing ERRORLEVEL tracking in debugger

REM Test 1: Successful command (ERRORLEVEL should be 0)
echo This command succeeds
echo After success, ERRORLEVEL should be 0

REM Test 2: Command that fails
findstr "NONEXISTENT" nonexistent_file.txt 2>nul
echo After failed findstr, ERRORLEVEL should be non-zero (typically 1)

REM Test 3: Explicit exit /b with code
call :return_code_5
echo After subroutine that returns 5, ERRORLEVEL should be 5

REM Test 4: Checking ERRORLEVEL in IF statement
call :return_code_10
if errorlevel 10 (
    echo ERRORLEVEL is 10 or greater
) else (
    echo ERRORLEVEL is less than 10
)

REM Test 5: Reset to 0
echo Success command
echo ERRORLEVEL should be back to 0

echo Test complete
exit /b 0

:return_code_5
echo Returning exit code 5
exit /b 5

:return_code_10
echo Returning exit code 10
exit /b 10
