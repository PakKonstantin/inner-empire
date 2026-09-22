; Uninstall cleanup for the Windows installer.
;
; Tauri's NSIS integration exposes four hook points — before and after
; install, before and after uninstall — and nothing that can add a page to
; the wizard. That shapes where the decisions live:
;
;   * The installer registers the *capability* only. `fileAssociations` in
;     the bundle config makes Inner Empire appear under "Open with" for
;     Markdown, which changes nothing about what opens when you double-click.
;     Becoming the default handler, and adding a folder context-menu entry,
;     are asked for in the application's own Settings — where the choice can
;     be seen, changed and undone, rather than in a wizard page seen once.
;
;   * This file therefore only cleans up. It removes what the application may
;     have written, and the cache, and nothing else.
;
; What is deliberately absent is as important as what is here: there is no
; path in this file that removes a vault, and none that removes
; %APPDATA%\InnerEmpire\data. Vaults are ordinary folders wherever the user
; put them; the uninstaller is never told where, and must never guess.

!include "LogicLib.nsh"

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Nothing. An installer that seizes .md is one people uninstall.
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; The keys the application writes when the user turns integration on in
  ; Settings. Per-user (HKCU) throughout, so none of it needed admin rights
  ; and none of it can affect another account.
  DeleteRegKey HKCU "Software\Classes\InnerEmpire.Markdown"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\InnerEmpireVault"

  ; Clear the extension only while it still points at us — someone else may
  ; own it by now, and taking that away would be the same rudeness in
  ; reverse.
  ReadRegStr $0 HKCU "Software\Classes\.md" ""
  ${If} $0 == "InnerEmpire.Markdown"
    DeleteRegValue HKCU "Software\Classes\.md" ""
  ${EndIf}
  ReadRegStr $0 HKCU "Software\Classes\.markdown" ""
  ${If} $0 == "InnerEmpire.Markdown"
    DeleteRegValue HKCU "Software\Classes\.markdown" ""
  ${EndIf}

  ; Tell the shell, or Explorer keeps the old icon until the next sign-in.
  System::Call 'shell32::SHChangeNotify(i 0x8000000, i 0, i 0, i 0)'

  ; The search index and thumbnails are rebuilt from the notes, so losing
  ; them costs nothing. This directory is the application's own.
  RMDir /r "$LOCALAPPDATA\InnerEmpire\cache"

  ; Settings, hotkeys, logs and the recent-vault list are left in place: a
  ; reinstall should find them, and an uninstaller that quietly deletes
  ; preferences is one nobody forgives. "Remove everything" belongs in the
  ; application, where it can say what it is about to do.
!macroend
