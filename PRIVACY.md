# Vitela privacy policy

_Last updated: 7 October 2026_

This policy covers **Vitela for Windows**, distributed through the Microsoft
Store, and **Vitela for Android**, distributed through Google Play.

## Short version

Vitela works entirely on your device. It does not collect or transmit
personal data, and it sends nothing to its developer or to any third party.
It contains no analytics, advertising, telemetry, crash reporting or account
system, and it makes no network connections. The Android app requests no
permissions at all, including internet access.

## What the app reads

- **The documents you open.** They are read and edited only on your device,
  and they are written only where you choose to save them. On Android, you
  choose them through the system file picker, "Open with", the share sheet
  or drag and drop, and Vitela can reach only the files you pick.
- **Your signing certificate**, only when you use **Sign**.
  - On Windows, you choose where it comes from: a certificate in your Windows
    user store, a `.pfx`/`.p12` file you pick, or a smart card or token
    through the driver you select.
  - On Android, it is a `.pfx`/`.p12` file you pick.

  The certificate is used to sign the document on your device. Vitela never
  stores it, never copies it elsewhere and never sends it anywhere.
- **Windows Recent items** (Windows only). Vitela adds the PDFs you open to
  the Windows *Recent* list and reads that list back to show them on its
  Recent screen. That list belongs to Windows, and you can clear it in the
  Windows settings.

## What the app stores

- **A small diagnostic log** (Windows only) of recent failures, kept on your
  device. Its entries contain the error category, the operation, the time
  and random identifiers. They never contain file paths, document content or
  passwords. The log is capped at 256 KB and is removed when you uninstall
  the app.
- **Your drawn signature** (Android only), and only if you choose to remember
  it. It is kept as an image in the app's private storage on that phone. It
  is excluded from cloud backups and from device-to-device transfers, so it
  never leaves the phone. You can forget it from the app at any time, and it
  is removed when you uninstall the app.

Passwords and PINs you type are kept in memory only while they are needed,
for example while a protected document is open. They are never written to
disk.

## Data shared with the stores

Microsoft handles the download, installation and updates of the Windows app
through the Microsoft Store, under the
[Microsoft Privacy Statement](https://privacy.microsoft.com/privacystatement).
Google handles the same for the Android app through Google Play, under the
[Google Privacy Policy](https://policies.google.com/privacy). Vitela does not
add anything to what either store collects.

## Children

Vitela collects no data from anyone, including children.

## Changes

If a future version changes any of the above, this page will be updated
before that version is released, and the date at the top will change.

## Contact

Questions about this policy can be asked on the project's
[issue tracker](https://github.com/PabloBaroParra/vitela/issues).
