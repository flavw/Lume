# LumeView installer 0.36.3

Microsoft Store EXE installer parameters: `/S`

Do not select the option stating that no silent switches are required.
Architecture: x64. Application languages: English, Portuguese, Spanish, French.
Interactive installer language: `--lang=en`, `--lang=pt`, `--lang=es`, `--lang=fr` (English by default).

## Exit codes

- 0: installation succeeded (also successful updates of an existing installation).
- 1602: user declined interactive installation.
- 1618: another LumeView installation is in progress.
- 112: insufficient disk space.
- 1603: installation failed.
- 1001: close the running LumeView application before updating.
- 87: invalid command-line arguments.

No app-already-exists code: existing installations are updated.
No reboot-required code: this installer does not request a restart.
No network-failure code: the complete payload is bundled; no network download is performed.
No separate security-policy-rejection code is emitted; if execution is blocked before startup, Windows handles it.
Map 1603, 1001 and 87 as miscellaneous installation failures in Partner Center.

`/S` suppresses confirmation, success and error message boxes. Return codes remain available to the caller.

Test-only extraction without registry or shortcuts:
`LumeView-Setup.exe /S --stage-only --target=C:\path\to\test-directory`

The installer and bundled PE files are not digitally signed. This must be resolved before submitting through the Microsoft Store EXE/MSI route.
