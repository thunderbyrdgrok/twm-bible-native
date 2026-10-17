; NSIS installer for TWM Bible (Windows x64).
; Build: makensis /DVERSION=1.2.0 /DSRCDIR=<repo root> /DEXE=<path to twm-bible-native.exe> /DOUTFILE=<out.exe> packaging\windows\installer.nsi
Unicode true
!include "MUI2.nsh"
!include "x64.nsh"

!ifndef VERSION
  !define VERSION "1.2.0"
!endif
!ifndef SRCDIR
  !define SRCDIR "..\.."
!endif
!ifndef EXE
  !define EXE "${SRCDIR}\target\release\twm-bible-native.exe"
!endif
!ifndef OUTFILE
  !define OUTFILE "${SRCDIR}\dist\TWM-Bible-${VERSION}-windows-x64-setup.exe"
!endif

!define APPNAME "TWM Bible"
!define REGKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\TWMBibleNative"

Name "${APPNAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\TWM Bible"
InstallDirRegKey HKLM "Software\TWMBibleNative" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!define MUI_ICON "${SRCDIR}\packaging\windows\icon.ico"
!define MUI_UNICON "${SRCDIR}\packaging\windows\icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\twm-bible-native.exe"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SRCDIR}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetRegView 64
  SetOutPath "$INSTDIR"
  File "/oname=twm-bible-native.exe" "${EXE}"
  File "/oname=twm-bible.ico" "${SRCDIR}\packaging\windows\icon.ico"
  File "${SRCDIR}\LICENSE"
  File "${SRCDIR}\fonts\OFL.txt"
  SetOutPath "$INSTDIR\data"
  File "${SRCDIR}\data\*.json"
  SetOutPath "$INSTDIR\assets"
  File "${SRCDIR}\assets\twmintro.mp4"
  SetOutPath "$INSTDIR"

  CreateShortCut "$SMPROGRAMS\TWM Bible.lnk" "$INSTDIR\twm-bible-native.exe" "" "$INSTDIR\twm-bible.ico"
  CreateShortCut "$DESKTOP\TWM Bible.lnk" "$INSTDIR\twm-bible-native.exe" "" "$INSTDIR\twm-bible.ico"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\TWMBibleNative" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${REGKEY}" "DisplayName" "${APPNAME}"
  WriteRegStr HKLM "${REGKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${REGKEY}" "Publisher" "thunderbyrdgrok"
  WriteRegStr HKLM "${REGKEY}" "DisplayIcon" "$INSTDIR\twm-bible.ico"
  WriteRegStr HKLM "${REGKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKLM "${REGKEY}" "NoModify" 1
  WriteRegDWORD HKLM "${REGKEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetRegView 64
  Delete "$SMPROGRAMS\TWM Bible.lnk"
  Delete "$DESKTOP\TWM Bible.lnk"
  RMDir /r "$INSTDIR\data"
  RMDir /r "$INSTDIR\assets"
  Delete "$INSTDIR\twm-bible-native.exe"
  Delete "$INSTDIR\twm-bible.ico"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\OFL.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "${REGKEY}"
  DeleteRegKey HKLM "Software\TWMBibleNative"
SectionEnd
