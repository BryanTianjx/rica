; ICA Personal NSIS installer hooks, referenced from tauri.conf.json
; (bundle.windows.nsis.installerHooks). Tauri inserts the hook macros into the
; generated installer.nsi and calls them at the fixed points.
;
; BROWSER-05: register / unregister the Native Messaging host "com.ica.connector"
; for Chrome and Edge. Everything is per-user (HKCU), matching INSTALLMODE
; "currentUser" / RequestExecutionLevel user: no elevation, no HKLM writes.
;
; Layout facts this file depends on (see scripts/packaging/stage_bundle.sh):
;   $INSTDIR\bundle-resources\sidecar\runtime\python\python.exe   interpreter
;   $INSTDIR\bundle-resources\sidecar\tools\write_connector_manifest.py
;   $INSTDIR\bundle-resources\sidecar\tools\connector_manifest.json
;   $INSTDIR\bundle-resources\sidecar\tools\ica_connector_host.cmd
;
; The manifest file itself lives in the per-user browser data dir, which is the
; app/browser/connector.py::host_manifest_path contract; the two registry keys
; point there. Keep this file pure ASCII: makensis reads it on any codepage.

; $R9 = browser data root: %LOCALAPPDATA%\ICA\data, or the ICA_DATA_DIR of the
; installer process when set (verify_installer.ps1 redirects installs to a
; throwaway data root this way).
!macro ICA_SetBrowserDataDir
  StrCpy $R9 "$LOCALAPPDATA\ICA\data"
  ReadEnvStr $R8 "ICA_DATA_DIR"
  StrCmp $R8 "" +2 0
  StrCpy $R9 $R8
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro ICA_SetBrowserDataDir
  StrCpy $R8 "$R9\browser\connector"
  DetailPrint "ICA: writing native messaging host manifest to $R8"
  ExecWait '"$INSTDIR\bundle-resources\sidecar\runtime\python\python.exe" "$INSTDIR\bundle-resources\sidecar\tools\write_connector_manifest.py" install --template "$INSTDIR\bundle-resources\sidecar\tools\connector_manifest.json" --launcher "$INSTDIR\bundle-resources\sidecar\tools\ica_connector_host.cmd" --out "$R8"' $R7
  DetailPrint "ICA: write_connector_manifest exit code $R7"
  WriteRegStr HKCU "Software\Google\Chrome\NativeMessagingHosts\com.ica.connector" "" "$R8\com.ica.connector.json"
  WriteRegStr HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.ica.connector" "" "$R8\com.ica.connector.json"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro ICA_SetBrowserDataDir
  StrCpy $R8 "$R9\browser\connector"
  DetailPrint "ICA: unregistering native messaging host (com.ica.connector)"
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.ica.connector"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.ica.connector"
  ; Only the installer-owned manifest; the pairing identity files next to it
  ; are user data and stay.
  Delete "$R8\com.ica.connector.json"
!macroend
