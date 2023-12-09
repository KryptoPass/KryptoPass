@echo off

pushd ..
vendor\Premake\Windows\premake5.exe --file=premake5.lua vs2022
popd
pause