@echo off
REM Test batch file for FOR loop debugging
REM This file tests all variants of FOR loops

echo ========================================
echo Testing FOR Loops
echo ========================================

REM Test 1: Basic FOR loop
echo.
echo Test 1: Basic FOR loop with items
FOR %%i IN (apple banana cherry) DO (
    echo   Item: %%i
)

REM Test 2: FOR /L numeric loop (counting up)
echo.
echo Test 2: FOR /L counting from 1 to 5
FOR /L %%n IN (1,1,5) DO (
    echo   Count: %%n
)

REM Test 3: FOR /L numeric loop (counting down)
echo.
echo Test 3: FOR /L counting from 10 to 5 by -1
FOR /L %%n IN (10,-1,5) DO (
    echo   Countdown: %%n
)

REM Test 4: FOR /L with step of 2
echo.
echo Test 4: FOR /L even numbers from 0 to 10
FOR /L %%n IN (0,2,10) DO (
    echo   Even: %%n
)

REM Test 5: Basic FOR loop with file wildcards
echo.
echo Test 5: FOR loop with file pattern
FOR %%f IN (*.bat) DO (
    echo   Batch file: %%f
)

REM Test 6: FOR loop with variables in items
echo.
echo Test 6: FOR loop using variables
SET ITEM1=first
SET ITEM2=second
SET ITEM3=third
FOR %%v IN (%ITEM1% %ITEM2% %ITEM3%) DO (
    echo   Variable item: %%v
)

REM Test 7: Nested FOR loops
echo.
echo Test 7: Nested FOR loops
FOR %%i IN (A B C) DO (
    FOR %%j IN (1 2 3) DO (
        echo   Outer: %%i, Inner: %%j
    )
)

REM Test 8: FOR loop with SET operations
echo.
echo Test 8: FOR loop with variable assignment
SET TOTAL=0
FOR /L %%n IN (1,1,5) DO (
    SET /A TOTAL+=%%n
)
echo   Total sum: %TOTAL%

REM Test 9: FOR /D directory listing
echo.
echo Test 9: FOR /D listing directories
echo   Directories in current folder:
FOR /D %%d IN (*) DO (
    echo     - %%d
)

REM Test 10: FOR loop with SETLOCAL scope
echo.
echo Test 10: FOR loop with SETLOCAL scope
SETLOCAL
SET SCOPE_VAR=outer
FOR %%s IN (inner) DO (
    SET SCOPE_VAR=%%s
    echo   Inside loop: SCOPE_VAR=%SCOPE_VAR%
)
ENDLOCAL
echo   After ENDLOCAL: SCOPE_VAR=%SCOPE_VAR%

echo.
echo ========================================
echo All FOR loop tests completed!
echo ========================================

exit /b 0
