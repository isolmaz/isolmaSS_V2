; ==============================================================================
; isolmaSS NSIS Installer Script
; Produces: target\release\isolmass-setup.exe (Budget: <= 3 MB)
; ==============================================================================

!define PRODUCT_NAME "isolmaSS"
!define PRODUCT_VERSION "0.3.0"
!define PRODUCT_PUBLISHER "isolmass"
!define PRODUCT_WEB_SITE "https://github.com/isolmass/betterSS"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\isolmass.exe"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_UNINST_ROOT_KEY "HKCU"

RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"

; MUI Configuration
!define MUI_ABORTWARNING

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

Section "MainSection" SEC01
  SetOutPath "$INSTDIR"
  SetOverwrite ifnewer
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
  SetAutoClose true
SectionEnd
