RustYu Legacy Test App
======================

This is a legacy non-MSI installer fixture for rust_yu.

Uninstall expectations:
- uninstall entry remains discoverable through standard Win32 registry keys
- uninstall and quiet uninstall both go through SpawnUninstall.ps1
- SpawnUninstall.ps1 exits quickly after creating UninstallWorker.ps1
- quiet uninstall is available through QuietUninstallString
- logs\leftover.log should remain after uninstall
- LocalAppData\RustYuLegacyTest\Data\leftover-user-profile.json should remain after uninstall
