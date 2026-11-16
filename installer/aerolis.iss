; Installateur d'Aerolis (Inno Setup 6) : installation pour l'utilisateur courant, sans droits
; administrateur. Compilation :  iscc /DAppVersion=1.0.0 installer\aerolis.iss
; L'exécutable attendu est target\release\aerolis.exe ; le résultat va dans target\installer\.
;
; Les composants (PDFium, ONNX Runtime, modèles OCR) ne sont pas inclus : l'application les
; télécharge depuis leur source officielle au premier lancement.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#define AppName "Aerolis"
#define AppExe "aerolis.exe"
#define ProgId "Aerolis.Document"
#define RepoUrl "https://github.com/padimato/aerolis"

[Setup]
AppId={{6F0B8C2E-4B7A-4E59-9C1D-3A5E7F2B9D41}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=padimato
AppPublisherURL={#RepoUrl}
AppSupportURL={#RepoUrl}/issues
AppUpdatesURL={#RepoUrl}/releases
DefaultDirName={autopf}\{#AppName}
; Même AppId que « Kite PDF » (ancien nom) : la mise à jour remplace l'ancienne installation, mais dans
; le nouveau dossier ; l'ancien dossier et ses raccourcis sont supprimés plus bas.
UsePreviousAppDir=no
UsePreviousGroup=no
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\target\installer
OutputBaseFilename=Aerolis-Setup-{#AppVersion}
SetupIconFile=..\assets\icon\aerolis.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
fr.DesktopIcon=Créer un raccourci sur le bureau
en.DesktopIcon=Create a desktop shortcut
fr.DefaultApp=Choisir Aerolis comme lecteur PDF par défaut (ouvre les Paramètres de Windows)
en.DefaultApp=Make Aerolis the default PDF reader (opens Windows Settings)
fr.LaunchApp=Lancer Aerolis
en.LaunchApp=Launch Aerolis
fr.PdfDocument=Document PDF
en.PdfDocument=PDF document
fr.RemoveUserData=Supprimer aussi vos réglages, l'historique des documents et les composants téléchargés (PDFium, OCR) ?
en.RemoveUserData=Also remove your settings, document history and downloaded components (PDFium, OCR)?

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; Flags: unchecked

[InstallDelete]
; Restes de l'installation sous l'ancien nom (« Kite PDF », versions 1.0 à 1.3.0).
Type: filesandordirs; Name: "{autopf}\Kite PDF"
Type: files; Name: "{autoprograms}\Kite PDF.lnk"
Type: files; Name: "{autodesktop}\Kite PDF.lnk"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; Inscriptions sous l'ancien nom (« Kite PDF »), remplacées par celles ci-dessous.
Root: HKA; Subkey: "Software\Classes\KitePDF.Document"; ValueType: none; Flags: deletekey
Root: HKA; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: none; ValueName: "KitePDF.Document"; Flags: deletevalue
Root: HKA; Subkey: "Software\Classes\Applications\kite-pdf.exe"; ValueType: none; Flags: deletekey
Root: HKA; Subkey: "Software\KitePDF"; ValueType: none; Flags: deletekey
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: none; ValueName: "Kite PDF"; Flags: deletevalue
; Type de document propre à l'application (icône et commande d'ouverture).
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "{cm:PdfDocument}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
; Proposée dans « Ouvrir avec » pour les .pdf, sans prendre la place du lecteur actuel.
Root: HKA; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".pdf"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
; Déclarée à Windows comme lecteur PDF possible (Paramètres > Applications par défaut).
Root: HKA; Subkey: "Software\Aerolis"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Aerolis\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Aerolis\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "{#AppName}"
Root: HKA; Subkey: "Software\Aerolis\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pdf"; ValueData: "{#ProgId}"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\Aerolis\Capabilities"; Flags: uninsdeletevalue

[Run]
Filename: "ms-settings:defaultapps?registeredAppUser={#AppName}"; Flags: shellexec nowait postinstall skipifsilent; Description: "{cm:DefaultApp}"
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent

[Code]
// Données de l'utilisateur : supprimées seulement s'il le demande.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) and not UninstallSilent then
  begin
    if MsgBox(CustomMessage('RemoveUserData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
    begin
      DelTree(ExpandConstant('{userappdata}\Aerolis'), True, True, True);
      DelTree(ExpandConstant('{localappdata}\Aerolis'), True, True, True);
    end;
  end;
end;
