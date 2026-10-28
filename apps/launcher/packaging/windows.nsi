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

; Current product identity.
!define APP_REGKEY "Software\Industrialis\Launcher"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisLauncher"
!define INSTALL_MARKER "install.json"
; Retired identity from older builds. Only used to migrate and clean up.
!define LEGACY_REGKEY "Software\Industrialis\GPUI"
!define LEGACY_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\IndustrialisGPUI"
!define LEGACY_INSTALL_MARKER "gpui-install.json"
!define LEGACY_FOLDER_NAME "Industrialis GPUI"

Name "Industrialis Launcher"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Industrialis Launcher"
InstallDirRegKey HKCU "${APP_REGKEY}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
VIProductVersion "${APPVERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "Industrialis Launcher"
VIAddVersionKey /LANG=1033 "FileDescription" "Industrialis Launcher Installer"
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
Var LegacyDir
Var MigrateLegacy

Function .onInit
  SetShellVarContext current
  StrCpy $MigrateLegacy "0"

  ; Older updaters pass /D=<existing directory>. If that directory is the
  ; retired default, install to the current one and remove the old copy.
  ${GetFileName} $R0 $INSTDIR
  ${GetParent} $R1 $INSTDIR
  ${If} $R0 == "${LEGACY_FOLDER_NAME}"
  ${AndIf} $R1 == "$LOCALAPPDATA\Programs"
    StrCpy $MigrateLegacy "1"
    StrCpy $LegacyDir "$INSTDIR"
    StrCpy $INSTDIR "$LOCALAPPDATA\Programs\Industrialis Launcher"
  ${EndIf}

  ; Without /D the directory comes from the registry. Adopt a custom location
  ; from an older build, but never inherit the retired default folder.
  ReadRegStr $R2 HKCU "${APP_REGKEY}" "InstallDir"
  ${If} $R2 == ""
    ReadRegStr $R2 HKCU "${LEGACY_REGKEY}" "InstallDir"
    ${If} $R2 != ""
      ${GetFileName} $R0 $R2
      ${GetParent} $R1 $R2
      ${If} $R0 == "${LEGACY_FOLDER_NAME}"
      ${AndIf} $R1 == "$LOCALAPPDATA\Programs"
        StrCpy $MigrateLegacy "1"
        StrCpy $LegacyDir "$R2"
      ${ElseIf} $INSTDIR == "$LOCALAPPDATA\Programs\Industrialis Launcher"
        StrCpy $INSTDIR "$R2"
      ${EndIf}
    ${EndIf}
  ${EndIf}

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
    ${If} ${FileExists} "$INSTDIR\${INSTALL_MARKER}"
    ${OrIf} ${FileExists} "$INSTDIR\${LEGACY_INSTALL_MARKER}"
      ; Existing installation marker found.
    ${Else}
      MessageBox MB_ICONSTOP "This is not an existing launcher installation. Use the normal installer." /SD IDOK
      SetErrorLevel 1
      Abort
    ${EndIf}
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
    MessageBox MB_ICONSTOP "Could not retire the original launcher. Close launcher windows and retry, or review the old installation. The new launcher was not started and your profile has not been moved or deleted." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
!endif
  FileOpen $R1 "$INSTDIR\${INSTALL_MARKER}" w
  FileWrite $R1 '{"format":1,"kind":"nsis"}'
  FileClose $R1
  Delete "$INSTDIR\${LEGACY_INSTALL_MARKER}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
!ifdef NOTICEFILE
  File /oname=THIRD_PARTY_NOTICES.txt "${NOTICEFILE}"
!endif
!ifdef LICENSEFILE
  File /oname=LICENSE "${LICENSEFILE}"
!endif
!ifndef SMOKE_TEST
  WriteRegStr HKCU "${APP_REGKEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Industrialis Launcher"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${APPVERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\industrialis.exe"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
!ifdef REGISTER_PROTOCOL
  ReadRegStr $R2 HKCU "Software\Classes\industrialislauncher\shell\open\command" ""
  StrCpy $R3 '$\"$INSTDIR\industrialis.exe$\" $\"%1$\"'
  ${If} $R2 != $R3
    WriteRegStr HKCU "${APP_REGKEY}" "ProtocolPreviousCommand" "$R2"
    ReadRegStr $R2 HKCU "Software\Classes\industrialislauncher" ""
    WriteRegStr HKCU "${APP_REGKEY}" "ProtocolPreviousName" "$R2"
  ${EndIf}
  WriteRegDWORD HKCU "${APP_REGKEY}" "ProtocolRegistered" 1
  WriteRegStr HKCU "Software\Classes\industrialislauncher" "" "URL:Industrialis Launcher"
  WriteRegStr HKCU "Software\Classes\industrialislauncher" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\industrialislauncher\shell\open\command" "" "$R3"
!endif
  ; Retire the old identity so no later install can inherit it.
  DeleteRegKey HKCU "${LEGACY_REGKEY}"
  DeleteRegKey HKCU "${LEGACY_UNINSTALL_KEY}"
  ; Remove the shortcut name used by older builds so upgrades do not leave duplicates.
  Delete "$SMPROGRAMS\Industrialis GPUI.lnk"
  CreateShortcut "$SMPROGRAMS\Industrialis Launcher.lnk" "$INSTDIR\industrialis.exe"
!endif
  Delete "$INSTDIR\industrialis.exe.previous"

  ; Move an older installation out of the retired default folder. Only known
  ; files are removed; the directory itself is kept if anything else remains.
  ${If} $MigrateLegacy == "1"
  ${AndIf} $LegacyDir != ""
  ${AndIf} $LegacyDir != $INSTDIR
    Delete "$LegacyDir\industrialis.exe"
    Delete "$LegacyDir\industrialis.exe.previous"
    Delete "$LegacyDir\${INSTALL_MARKER}"
    Delete "$LegacyDir\${LEGACY_INSTALL_MARKER}"
    Delete "$LegacyDir\Uninstall.exe"
    Delete "$LegacyDir\THIRD_PARTY_NOTICES.txt"
    Delete "$LegacyDir\LICENSE"
    RMDir "$LegacyDir"
  ${EndIf}
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
  Delete "$INSTDIR\${INSTALL_MARKER}"
  Delete "$INSTDIR\${LEGACY_INSTALL_MARKER}"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.txt"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
!ifndef SMOKE_TEST
  ReadRegStr $R0 HKCU "${APP_REGKEY}" "InstallDir"
  ${If} $R0 == $INSTDIR
    ; Updater builds must also clean a protocol registered by an earlier
    ; replacement build. Restore the previous handler only while we still own it.
    ReadRegDWORD $R1 HKCU "${APP_REGKEY}" "ProtocolRegistered"
    ReadRegStr $R2 HKCU "Software\Classes\industrialislauncher\shell\open\command" ""
    StrCpy $R3 '$\"$INSTDIR\industrialis.exe$\" $\"%1$\"'
    ${If} $R1 == 1
    ${AndIf} $R2 == $R3
      ReadRegStr $R2 HKCU "${APP_REGKEY}" "ProtocolPreviousCommand"
      ${If} $R2 != ""
        WriteRegStr HKCU "Software\Classes\industrialislauncher\shell\open\command" "" "$R2"
        ReadRegStr $R2 HKCU "${APP_REGKEY}" "ProtocolPreviousName"
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
    DeleteRegKey HKCU "${APP_REGKEY}"
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
  ${EndIf}
  ; The retired identity is always removed, even if $INSTDIR moved.
  DeleteRegKey HKCU "${LEGACY_REGKEY}"
  DeleteRegKey HKCU "${LEGACY_UNINSTALL_KEY}"
  Delete "$SMPROGRAMS\Industrialis Launcher.lnk"
  Delete "$SMPROGRAMS\Industrialis GPUI.lnk"
!endif
  ; Never recursively delete this directory or the user's app-data directory.
  RMDir "$INSTDIR"
SectionEnd
