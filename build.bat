@echo off
rem Packs the mod into target\mogged-<version>.hnmod (needs Python 3.11+).
rem Horizon's launcher compiles it into the game: put the .hnmod in <instance>\horizon-native-mods\ and press Build.
where py >nul 2>nul && (py -3 "%~dp0tools\package.py" %*) || (python "%~dp0tools\package.py" %*)
