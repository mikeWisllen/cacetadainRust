@echo off
echo Compilando driver de kernel...

REM Definir variáveis de ambiente para o WDK
SET WDK_PATH=C:\Program Files (x86)\Windows Kits\10\
SET INC_PATH=%WDK_PATH%Include\10.0.19041.0
SET LIB_PATH=%WDK_PATH%Lib\10.0.19041.0
SET BIN_PATH=%WDK_PATH%bin\10.0.19041.0\x64

REM Verificar se as ferramentas estão presentes
if not exist "%BIN_PATH%\cl.exe" (
    echo Erro: Ferramentas de compilação não encontradas.
    echo Instale o Windows Driver Kit (WDK) primeiro.
    exit /b 1
)

REM Criar diretório de saída
if not exist "build" mkdir build

REM Compilar driver
"%BIN_PATH%\cl.exe" /I"%INC_PATH%\km\crt" /I"%INC_PATH%\shared" /I"%INC_PATH%\km" /Zi /W4 /WX /Od /Oy- /Gz /GS- /FI"ntdefs.h" /FI"ntifs.h" /FI"ntstrsafe.h" /FI"ntddk.h" /D_X86_=1 /D_AMD64_=1 /DWIN64=1 /Di386=1 /DNDEBUG /D_WIN32_WINNT=0x0603 /DMSC_NOOPT /DNTDDI_VERSION=0x06030000 /DKMDF_MAJOR_VERSION=01 /DKMDF_MINOR_VERSION=015 /c driver.c /Fodriver.obj

if %errorlevel% neq 0 (
    echo Erro na compilação. Abortando.
    exit /b %errorlevel%
)

REM Linkar driver
"%BIN_PATH%\link.exe" /OUT:build\driver.sys /INCREMENTAL:NO /NOLOGO /NODEFAULTLIB /ENTRY:DriverEntry /SUBSYSTEM:NATIVE /DRIVER /MERGE:.rdata=.text /MERGE:.pdata=.text /INTEGRITYCHECK /RELEASE /DYNAMICBASE /NXCOMPAT /LTCG driver.obj %LIB_PATH%\km\x64\ntoskrnl.lib %LIB_PATH%\km\x64\hal.lib %LIB_PATH%\km\x64\wmilib.lib

if %errorlevel% neq 0 (
    echo Erro no link. Abortando.
    exit /b %errorlevel%
)

REM Assinar o driver (requer certificado)
REM "%BIN_PATH%\signtool.exe" sign /f certificate.pfx /p password /t http://timestamp.digicert.com build\driver.sys

echo Compilação concluída com sucesso!
echo O driver está disponível em build\driver.sys