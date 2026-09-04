; ==============================================================================
; isolmaSS NSIS Installer Script
; Produces: target\release\isolmass-setup.exe (Budget: <= 3 MB)
; ==============================================================================

!define PRODUCT_NAME "isolmaSS"
!ifndef PRODUCT_VERSION
  !error "PRODUCT_VERSION must be supplied by package.bat"
!endif
!define PRODUCT_PUBLISHER "isolmaSS"
!define PRODUCT_WEB_SITE "https://github.com/isolmaz/isolmaSS_V2"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\isolmass.exe"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_UNINST_ROOT_KEY "HKCU"

RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

Var IsUpdate

; MUI Configuration
!define MUI_ABORTWARNING
!define MUI_ICON "resources\app.ico"
!define MUI_UNICON "resources\app.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\isolmass.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch isolmaSS now"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "target\release\isolmass-setup.exe"
InstallDir "$LOCALAPPDATA\isolmaSS"
InstallDirRegKey HKCU "${PRODUCT_DIR_REGKEY}" ""
ShowInstDetails show
ShowUnInstDetails show

Function .onInit
  StrCpy $IsUpdate "0"
  ${GetParameters} $R0
  ${GetOptions} $R0 "/UPDATE" $R1
  ${If} $R1 != ""
    StrCpy $IsUpdate "1"
    Sleep 1500
  ${EndIf}
FunctionEnd

Section "MainSection" SEC01
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File "target\release\isolmass.exe"

  ; Start Menu Shortcuts
  CreateDirectory "$SMPROGRAMS\isolmaSS"
  CreateShortcut "$SMPROGRAMS\isolmaSS\isolmaSS.lnk" "$INSTDIR\isolmass.exe"
  CreateShortcut "$SMPROGRAMS\isolmaSS\Uninstall.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

Section -AdditionalIcons
  WriteIniStr "$INSTDIR\${PRODUCT_NAME}.url" "InternetShortcut" "URL" "${PRODUCT_WEB_SITE}"
  CreateShortcut "$SMPROGRAMS\isolmaSS\Website.lnk" "$INSTDIR\${PRODUCT_NAME}.url"
SectionEnd

Section -Post
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${PRODUCT_DIR_REGKEY}" "" "$INSTDIR\isolmass.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayName" "$(^Name)"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\isolmass.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"

  ${If} $IsUpdate == "1"
    Exec '"$INSTDIR\isolmass.exe"'
  ${EndIf}
SectionEnd

Section Uninstall
  Delete "$INSTDIR\${PRODUCT_NAME}.url"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\isolmass.exe"

  Delete "$SMPROGRAMS\isolmaSS\Website.lnk"
  Delete "$SMPROGRAMS\isolmaSS\Uninstall.lnk"
  Delete "$SMPROGRAMS\isolmaSS\isolmaSS.lnk"
  RMDir "$SMPROGRAMS\isolmaSS"

  RMDir "$INSTDIR"

  DeleteRegKey ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKCU "${PRODUCT_DIR_REGKEY}"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "isolmaSS"
  SetAutoClose true
SectionEnd
