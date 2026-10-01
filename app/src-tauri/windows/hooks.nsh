; Página con checkbox para dejar o no devkit-mcp.exe.
; El exe viaja en el instalador; si la casilla está desmarcada, se borra al final.
!include nsDialogs.nsh
!include LogicLib.nsh

Var DevkitInstallMcp
Var DevkitMcpCheckbox

Function DevkitMcpPagePre
  ${If} $PassiveMode = 1
    StrCpy $DevkitInstallMcp "1"
    Abort
  ${EndIf}
FunctionEnd

Function DevkitMcpPageCreate
  !insertmacro MUI_HEADER_TEXT "DevKit MCP" "Opcional"
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}
  ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

  ${NSD_CreateLabel} 0 0 100% 36u "Permite ver los logs de DevKit desde Cursor u otros agentes."
  Pop $1

  ${NSD_CreateCheckbox} 0 48u 100% 12u "Instalar DevKit MCP"
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
  DetailPrint "Si hay internet se descargan Nginx, PHP, MariaDB y Composer. Si no, se usan los del instalador."
  System::Call 'kernel32::SetEnvironmentVariable(t "DEVKIT_ROOT", t "$INSTDIR")'
  nsExec::ExecToLog '"$INSTDIR\devkit.exe" setup'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION "No se pudo instalar Nginx, PHP, MariaDB y Composer.$\nAbre DevKit y usa Setup completo, o revisa la carpeta logs."
  ${EndIf}
  StrCmp $DevkitInstallMcp "1" devkit_mcp_keep
    Delete "$INSTDIR\devkit-mcp.exe"
  devkit_mcp_keep:
!macroend
