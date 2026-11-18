#ifndef AppVersion
  #error Pass /DAppVersion from build-installer.ps1
#endif

[Setup]
; Unique ID for Refined updates and uninstallation
AppId={{8B1D4F37-2E4A-498C-B819-21F5A6740C33}}
AppName=Refined
AppVersion={#AppVersion}
AppPublisher=Nikita Berger
AppPublisherURL=https://github.com/Nikityyy/refined
AppSupportURL=https://github.com/Nikityyy/refined/issues
AppUpdatesURL=https://github.com/Nikityyy/refined/releases/latest
DefaultDirName={localappdata}\Programs\Refined
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern dynamic
SetupIconFile=refined.ico
UninstallDisplayIcon={app}\Refined.exe
DisableWelcomePage=yes
DisableDirPage=no
DisableProgramGroupPage=yes
DisableReadyPage=yes
UsePreviousTasks=yes
UsePreviousAppDir=yes
ShowLanguageDialog=no
AppMutex=Local\Refined
SetupMutex=Local\RefinedSetup
CloseApplications=no
RestartApplications=no
Uninstallable=yes
Compression=lzma2/ultra64
SolidCompression=yes
OutputDir=..\target\installer
OutputBaseFilename=Refined-Setup-{#AppVersion}
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Refined installieren

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[Messages]
english.SelectDirLabel3=Refined installs for your Windows account only. No administrator access is needed.
english.SelectTasksDesc=Shortcuts
english.SelectTasksLabel2=Refined will be available in the Start menu. Would you also like a desktop shortcut?
english.SetupAppRunningError=Refined is currently running in the background or system tray.%n%nPlease exit Refined before proceeding, then click OK to continue or Cancel to exit.
english.UninstallAppRunningError=Refined is currently running in the background or system tray.%n%nPlease exit Refined before proceeding, then click OK to continue or Cancel to exit.
english.FinishedLabel=Refined is installed.%n%nSelect any text in any application and press Alt + Q to refine your writing.
english.ConfirmUninstall=Remove Refined?%n%nYour study history and settings will be preserved.
german.SelectDirDesc=Wo soll Refined installiert werden?
german.SelectDirLabel3=Refined wird nur für dein Windows-Konto installiert. Administratorrechte sind nicht nötig.
german.SelectTasksDesc=Verknüpfungen
german.SelectTasksLabel2=Im Startmenü findest du Refined nach der Installation. Möchtest du auch eine Verknüpfung auf dem Desktop?
german.SetupAppRunningError=Refined läuft noch im Hintergrund oder im Infobereich.%n%nBitte beende Refined und klicke auf OK, um fortzufahren.
german.UninstallAppRunningError=Refined läuft noch im Hintergrund oder im Infobereich.%n%nBitte beende Refined und klicke auf OK, um fortzufahren.
german.FinishedLabel=Refined ist installiert.%n%nMarkiere Text in beliebigen Apps und drücke Alt + Q, um Texte zu verfeinern.
german.ConfirmUninstall=Möchtest du Refined entfernen?%n%nDeine Einstellungen und Historie bleiben erhalten.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"

[InstallDelete]
Type: files; Name: "{userdesktop}\Refined.lnk"; Tasks: not desktopicon

[Files]
Source: "..\target\release\refined.exe"; DestDir: "{app}"; DestName: "Refined.exe"; Flags: ignoreversion
Source: "refined.ico"; DestDir: "{app}"; DestName: "Refined.ico"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Refined"; Filename: "{app}\Refined.exe"; WorkingDir: "{app}"; IconFilename: "{app}\Refined.ico"
Name: "{userdesktop}\Refined"; Filename: "{app}\Refined.exe"; WorkingDir: "{app}"; Tasks: desktopicon; IconFilename: "{app}\Refined.ico"

[Run]
Filename: "{app}\Refined.exe"; Description: "{cm:LaunchProgram,Refined}"; Flags: nowait postinstall skipifsilent

[Code]
procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and RegValueExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Refined') then
    RegWriteStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Refined', '"' + ExpandConstant('{app}\Refined.exe') + '" --hidden');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Refined', Command) then
      if CompareText(Command, '"' + ExpandConstant('{app}\Refined.exe') + '" --hidden') = 0 then
        RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Refined');
  end;
end;
