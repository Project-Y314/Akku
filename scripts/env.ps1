# Dot-source this file when the VS developer environment is not registered with
# vswhere (the Akku build machine uses a portable Build Tools layout).
$vs = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
$msvc = Get-ChildItem "$vs\VC\Tools\MSVC" -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
$sdk = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\Lib" -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1

if ($msvc) {
    $env:PATH = "$($msvc.FullName)\bin\Hostx64\x64;$env:PATH"
    $env:LIB = "$($msvc.FullName)\lib\x64;$env:LIB"
}
if ($sdk) {
    $env:LIB = "$($sdk.FullName)\ucrt\x64;$($sdk.FullName)\um\x64;$env:LIB"
}
