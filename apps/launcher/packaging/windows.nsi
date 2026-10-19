Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!ifndef APPVERSION
  !error "APPVERSION is required"
!endif
!ifndef BINARY
  !error "BINARY is required"
!endif
!ifndef OUTFILE
  !error "OUTFILE is required"
!endif

Name "Industrialis Launcher (GPUI)"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Industrialis GPUI"
InstallDirRegKey HKCU "Software\Industrialis\GPUI" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
VIProductVersion "${APPVERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "Industrialis Launcher (GPUI)"
VIAddVersionKey /LANG=1033 "FileDescription" "Industrialis GPUI Installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${APPVERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Industrialis"
!define MUI_ABORTWARNING
!define MUI_ICON "${ICONFILE}"
!define MUI_UNICON "${ICONFILE}"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\industrialis.exe"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var UpdatePID
Var ReadyFile
Var ProcessHandle

Function .onInit
  SetShellVarContext current
  ${GetParameters} $R0
  ${GetOptions} $R0 "/UPDATEPID=" $UpdatePID
  ${If} $UpdatePID != ""
    SetSilent silent
    ${GetOptions} $R0 "/READYFILE=" $ReadyFile
    ${If} $ReadyFile == ""
      MessageBox MB_ICONSTOP "The update readiness path is missing. The launcher was kept open." /SD IDOK
      SetErrorLevel 1
      Abort
    ${EndIf}
    IfFileExists "$INSTDIR\gpui-install.json" +4 0
      MessageBox MB_ICONSTOP "This is not a GPUI installation. Use the normal installer." /SD IDOK
      SetErrorLevel 1
      Abort
    System::Call 'kernel32::OpenProcess(i 0x00100000, i 0, i $UpdatePID) p .r0'
    StrCpy $ProcessHandle $0
    ${If} $ProcessHandle == 0
      MessageBox MB_ICONSTOP "Could not wait for the launcher to exit. It was kept open." /SD IDOK
      SetErrorLevel 1
      Abort
    ${EndIf}
    ClearErrors
    FileOpen $R1 "$ReadyFile" w
    FileWrite $R1 "ready"
    FileClose $R1
    ${If} ${Errors}
      System::Call 'kernel32::CloseHandle(p $ProcessHandle)'
      SetErrorLevel 1
      Abort
    ${EndIf}
    System::Call 'kernel32::WaitForSingleObject(p $ProcessHandle, i 30000) i .r1'
    System::Call 'kernel32::CloseHandle(p $ProcessHandle)'
    Delete "$ReadyFile"
    ${If} $1 != 0
      MessageBox MB_ICONSTOP "The launcher is still open. Close it and retry the update." /SD IDOK
      SetErrorLevel 1
      Abort
    ${EndIf}
  ${EndIf}
FunctionEnd

Section "Launcher"
  SetOutPath "$INSTDIR"
  ; Keep a rollback executable until the new one is fully extracted.
  Delete "$INSTDIR\industrialis.exe.previous"
  IfFileExists "$INSTDIR\industrialis.exe" 0 extract
  ClearErrors
  Rename "$INSTDIR\industrialis.exe" "$INSTDIR\industrialis.exe.previous"
  ${If} ${Errors}
    MessageBox MB_ICONSTOP "Close other launcher windows and retry. No files were replaced." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
extract:
  ClearErrors
  File /oname=industrialis.exe "${BINARY}"
  ${If} ${Errors}
    Delete "$INSTDIR\industrialis.exe"
    Rename "$INSTDIR\industrialis.exe.previous" "$INSTDIR\industrialis.exe"
    MessageBox MB_ICONSTOP "Could not install the launcher. The previous executable was restored." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
!ifdef RETIRE_SCRIPT_FILE
  ; Verify/extract the replacement before retiring the original. Wait for normal
  ; close and delegate binary/shortcut removal to its own Squirrel updater.
  InitPluginsDir
  File /oname=$PLUGINSDIR\retire-electron.ps1 "${RETIRE_SCRIPT_FILE}"
  ExecWait '$\"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe$\" -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File $\"$PLUGINSDIR\retire-electron.ps1$\" -Apply' $R4
  ${If} $R4 != 0
    Delete "$INSTDIR\industrialis.exe"
    Rename "$INSTDIR\industrialis.exe.previous" "$INSTDIR\industrialis.exe"
    MessageBox MB_ICONSTOP "Could not retire the original launcher. Close launcher windows and retry, or review the old installation. GPUI was not started and your profile has not been moved or deleted." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
!endif
  FileOpen $R1 "$INSTDIR\gpui-install.json" w
  FileWrite $R1 '{"format":1,"kind":"nsis"}'
  FileClose $R1
  WriteUninstaller "$INSTDIR\Uninstall.exe"
!ifdef NOTICEFILE
  File /oname=THIRD_PARTY_NOTICES.txt "${NOTICEFILE}"
!endif
!ifdef LICENSEFILE
  File /oname=LICENSE "${LICENSEFILE}"
!endif
!ifndef SMOKE_TEST
  WriteRegStr HKCU "Software\Industrialis\GPUI" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI" "DisplayName" "Industrialis Launcher (GPUI)"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI" "DisplayVersion" "${APPVERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI" "DisplayIcon" "$INSTDIR\industrialis.exe"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI" "NoRepair" 1
!ifdef REGISTER_PROTOCOL
  ReadRegStr $R2 HKCU "Software\Classes\industrialislauncher\shell\open\command" ""
  StrCpy $R3 '$\"$INSTDIR\industrialis.exe$\" $\"%1$\"'
  ${If} $R2 != $R3
    WriteRegStr HKCU "Software\Industrialis\GPUI" "ProtocolPreviousCommand" "$R2"
    ReadRegStr $R2 HKCU "Software\Classes\industrialislauncher" ""
    WriteRegStr HKCU "Software\Industrialis\GPUI" "ProtocolPreviousName" "$R2"
  ${EndIf}
  WriteRegDWORD HKCU "Software\Industrialis\GPUI" "ProtocolRegistered" 1
  WriteRegStr HKCU "Software\Classes\industrialislauncher" "" "URL:Industrialis Launcher"
  WriteRegStr HKCU "Software\Classes\industrialislauncher" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\industrialislauncher\shell\open\command" "" "$R3"
!endif
  CreateShortcut "$SMPROGRAMS\Industrialis GPUI.lnk" "$INSTDIR\industrialis.exe"
!endif
  Delete "$INSTDIR\industrialis.exe.previous"
SectionEnd

Function .onInstSuccess
  ${If} $UpdatePID != ""
    Exec '"$INSTDIR\industrialis.exe"'
  ${EndIf}
FunctionEnd

Section "Uninstall"
  SetShellVarContext current
  IfFileExists "$INSTDIR\industrialis.exe" 0 app_already_removed
  ClearErrors
  Delete "$INSTDIR\industrialis.exe"
  ${If} ${Errors}
    MessageBox MB_ICONSTOP "Close the launcher before uninstalling. Your data has not been removed." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
app_already_removed:
  Delete "$INSTDIR\industrialis.exe.previous"
  Delete "$INSTDIR\gpui-install.json"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.txt"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
!ifndef SMOKE_TEST
  ReadRegStr $R0 HKCU "Software\Industrialis\GPUI" "InstallDir"
  ${If} $R0 == $INSTDIR
    ; Updater builds must also clean a protocol registered by an earlier
    ; replacement build. Restore the previous handler only while we still own it.
    ReadRegDWORD $R1 HKCU "Software\Industrialis\GPUI" "ProtocolRegistered"
    ReadRegStr $R2 HKCU "Software\Classes\industrialislauncher\shell\open\command" ""
    StrCpy $R3 '$\"$INSTDIR\industrialis.exe$\" $\"%1$\"'
    ${If} $R1 == 1
    ${AndIf} $R2 == $R3
      ReadRegStr $R2 HKCU "Software\Industrialis\GPUI" "ProtocolPreviousCommand"
      ${If} $R2 != ""
        WriteRegStr HKCU "Software\Classes\industrialislauncher\shell\open\command" "" "$R2"
        ReadRegStr $R2 HKCU "Software\Industrialis\GPUI" "ProtocolPreviousName"
        WriteRegStr HKCU "Software\Classes\industrialislauncher" "" "$R2"
      ${Else}
        DeleteRegValue HKCU "Software\Classes\industrialislauncher\shell\open\command" ""
        DeleteRegKey /ifempty HKCU "Software\Classes\industrialislauncher\shell\open\command"
        DeleteRegKey /ifempty HKCU "Software\Classes\industrialislauncher\shell\open"
        DeleteRegKey /ifempty HKCU "Software\Classes\industrialislauncher\shell"
        DeleteRegValue HKCU "Software\Classes\industrialislauncher" ""
        DeleteRegValue HKCU "Software\Classes\industrialislauncher" "URL Protocol"
        DeleteRegKey /ifempty HKCU "Software\Classes\industrialislauncher"
      ${EndIf}
    ${EndIf}
    Delete "$SMPROGRAMS\Industrialis GPUI.lnk"
    DeleteRegKey HKCU "Software\Industrialis\GPUI"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI"
  ${EndIf}
!endif
  ; Never recursively delete this directory or the user's app-data directory.
  RMDir "$INSTDIR"
SectionEnd
