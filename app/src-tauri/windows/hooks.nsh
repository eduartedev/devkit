; Página con checkbox para dejar o no devkit-mcp.exe.
; El exe viaja en el instalador; si la casilla está desmarcada, se borra al final.
!include nsDialogs.nsh
!include LogicLib.nsh

Var DevkitInstallMcp
Var DevkitMcpCheckbox
Var DevkitMcpSubtitle
Var DevkitMcpBody
Var DevkitMcpCheck
Var DevkitSetupDetail
Var DevkitSetupFail

!define DEVKIT_LANG_ENGLISH 1033

Function DevkitLoadStrings
  ${If} $LANGUAGE == ${DEVKIT_LANG_ENGLISH}
    StrCpy $DevkitMcpSubtitle "Optional"
    StrCpy $DevkitMcpBody "Lets you view DevKit logs from Cursor or other agents."
    StrCpy $DevkitMcpCheck "Install DevKit MCP"
    StrCpy $DevkitSetupDetail "If there is internet, Nginx, PHP, MariaDB, Composer and Node are downloaded. Otherwise the copies in the installer are used."
    StrCpy $DevkitSetupFail "Could not install Nginx, PHP, MariaDB, Composer and Node.$\nOpen DevKit and use Full setup, or check the logs folder."
  ${Else}
    StrCpy $DevkitMcpSubtitle "Opcional"
    StrCpy $DevkitMcpBody "Permite ver los logs de DevKit desde Cursor u otros agentes."
    StrCpy $DevkitMcpCheck "Instalar DevKit MCP"
    StrCpy $DevkitSetupDetail "Si hay internet se descargan Nginx, PHP, MariaDB, Composer y Node. Si no, se usan los del instalador."
    StrCpy $DevkitSetupFail "No se pudo instalar Nginx, PHP, MariaDB, Composer y Node.$\nAbre DevKit y usa Setup completo, o revisa la carpeta logs."
  ${EndIf}
FunctionEnd

Function DevkitMcpPagePre
  ${If} $PassiveMode = 1
    StrCpy $DevkitInstallMcp "1"
    Abort
  ${EndIf}
FunctionEnd

Function DevkitMcpPageCreate
  Call DevkitLoadStrings
  !insertmacro MUI_HEADER_TEXT "DevKit MCP" "$DevkitMcpSubtitle"
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}
  ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

  ${NSD_CreateLabel} 0 0 100% 36u "$DevkitMcpBody"
  Pop $1

  ${NSD_CreateCheckbox} 0 48u 100% 12u "$DevkitMcpCheck"
  Pop $DevkitMcpCheckbox

  ${If} $DevkitInstallMcp == "0"
    ${NSD_Uncheck} $DevkitMcpCheckbox
  ${Else}
    ${NSD_Check} $DevkitMcpCheckbox
  ${EndIf}

  ${NSD_SetFocus} $DevkitMcpCheckbox
  nsDialogs::Show
FunctionEnd

Function DevkitMcpPageLeave
  ${NSD_GetState} $DevkitMcpCheckbox $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $DevkitInstallMcp "1"
  ${Else}
    StrCpy $DevkitInstallMcp "0"
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  ${If} $DevkitInstallMcp == ""
    StrCpy $DevkitInstallMcp "1"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  Call DevkitLoadStrings
  DetailPrint $DevkitSetupDetail
  System::Call 'kernel32::SetEnvironmentVariable(t "DEVKIT_ROOT", t "$INSTDIR")'
  nsExec::ExecToLog '"$INSTDIR\devkit.exe" setup'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION $DevkitSetupFail
  ${EndIf}
  StrCmp $DevkitInstallMcp "1" devkit_mcp_keep
    Delete "$INSTDIR\devkit-mcp.exe"
  devkit_mcp_keep:
!macroend
