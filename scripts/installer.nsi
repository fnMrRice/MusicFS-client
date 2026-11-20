Unicode true
RequestExecutionLevel user
!include "MUI2.nsh"
!include "x64.nsh"

Name "MusicFS"
OutFile "${SETUP_OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\MusicFS"
InstallDirRegKey HKCU "Software\MusicFS\Desktop" "InstallDir"
Icon "${APP_ICON}"
UninstallIcon "${APP_ICON}"
VIProductVersion "${PRODUCT_VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "MusicFS"
VIAddVersionKey /LANG=1033 "FileDescription" "MusicFS Desktop Installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${PRODUCT_VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "MusicFS contributors"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Function .onInit
    ${IfNot} ${RunningX64}
        MessageBox MB_ICONSTOP "MusicFS requires 64-bit Windows."
        Abort
    ${EndIf}
FunctionEnd

Section "MusicFS"
    SetShellVarContext current
    SetOutPath "$INSTDIR"
    File /oname=MusicFS.exe "${APP_BINARY}"
    File /oname=vcruntime140.dll "${VC_RUNTIME}"
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    CreateShortcut "$SMPROGRAMS\MusicFS.lnk" "$INSTDIR\MusicFS.exe"
    WriteRegStr HKCU "Software\MusicFS\Desktop" "InstallDir" "$INSTDIR"
    !define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\MusicFSDesktop"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "MusicFS Desktop"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "MusicFS"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\MusicFS.exe"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
SectionEnd

Section "Uninstall"
    SetShellVarContext current
    Delete "$SMPROGRAMS\MusicFS.lnk"
    Delete "$INSTDIR\MusicFS.exe"
    Delete "$INSTDIR\vcruntime140.dll"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
    DeleteRegKey HKCU "Software\MusicFS\Desktop"
SectionEnd
