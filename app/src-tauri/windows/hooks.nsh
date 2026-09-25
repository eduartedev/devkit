; Pregunta si se deja devkit-mcp.exe. El binario viaja en el instalador;
; si la respuesta es no, se borra al terminar de copiar archivos.
Var DevkitInstallMcp

!macro NSIS_HOOK_PREINSTALL
  IfSilent devkit_mcp_yes
  MessageBox MB_YESNO|MB_ICONQUESTION "Instalar tambien devkit-mcp.exe para que Cursor lea los logs de DevKit?" IDYES devkit_mcp_yes
  StrCpy $DevkitInstallMcp "0"
  Goto devkit_mcp_asked
  devkit_mcp_yes:
    StrCpy $DevkitInstallMcp "1"
  devkit_mcp_asked:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  StrCmp $DevkitInstallMcp "1" devkit_mcp_keep
    Delete "$INSTDIR\devkit-mcp.exe"
  devkit_mcp_keep:
!macroend
