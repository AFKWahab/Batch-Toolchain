@echo off
REM Test various IF statement types

echo Testing IF statements...
echo.

REM Test 1: IF ERRORLEVEL
echo === Test 1: IF ERRORLEVEL ===
cmd /c exit 5
IF ERRORLEVEL 5 echo PASS: ERRORLEVEL 5 detected (exit code 5)
IF ERRORLEVEL 6 echo FAIL: ERRORLEVEL 6 should not trigger
IF NOT ERRORLEVEL 10 echo PASS: NOT ERRORLEVEL 10 works
echo.

REM Test 2: IF string comparison
echo === Test 2: IF string comparison ===
SET TEST_VAR=hello
IF "%TEST_VAR%"=="hello" echo PASS: String equals works
IF "%TEST_VAR%"=="HELLO" echo PASS: Case-insensitive comparison
IF NOT "%TEST_VAR%"=="world" echo PASS: NOT string equals works
echo.

REM Test 3: IF EXIST
echo === Test 3: IF EXIST ===
echo test > temp_test_file.txt
IF EXIST temp_test_file.txt echo PASS: File exists
IF NOT EXIST nonexistent.txt echo PASS: File does not exist
del temp_test_file.txt
echo.

REM Test 4: IF DEFINED
echo === Test 4: IF DEFINED ===
SET DEFINED_VAR=value
IF DEFINED DEFINED_VAR echo PASS: Variable is defined
IF NOT DEFINED UNDEFINED_VAR echo PASS: Variable is not defined
echo.

REM Test 5: IF numeric comparisons (EQU, NEQ, LSS, LEQ, GTR, GEQ)
echo === Test 5: IF numeric comparisons ===
SET NUM1=10
SET NUM2=20
IF %NUM1% EQU 10 echo PASS: EQU works (10 == 10)
IF %NUM1% NEQ %NUM2% echo PASS: NEQ works (10 != 20)
IF %NUM1% LSS %NUM2% echo PASS: LSS works (10 < 20)
IF %NUM1% LEQ %NUM2% echo PASS: LEQ works (10 <= 20)
IF %NUM2% GTR %NUM1% echo PASS: GTR works (20 > 10)
IF %NUM2% GEQ %NUM1% echo PASS: GEQ works (20 >= 10)
echo.

REM Test 6: Combined with variables
echo === Test 6: Variables in conditions ===
SET COUNTER=5
IF %COUNTER% EQU 5 echo PASS: COUNTER is 5
IF %COUNTER% LEQ 10 echo PASS: COUNTER is less than or equal to 10
echo.

REM Test 7: IF with NOT modifier
echo === Test 7: IF with NOT modifier ===
SET VALUE=0
IF NOT %VALUE% EQU 1 echo PASS: NOT modifier works
cmd /c exit 0
IF NOT ERRORLEVEL 1 echo PASS: NOT ERRORLEVEL works
echo.

echo All IF statement tests completed!
