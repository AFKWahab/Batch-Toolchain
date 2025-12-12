@echo off
echo TestValue> temp_input.txt
SET /P USERNAME=<temp_input.txt
del temp_input.txt
echo Done
