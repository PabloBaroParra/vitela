# Distributing the macOS app

Vitela for Mac ships through two channels, both built from the same
`build/Vitela.app` (`scripts/build-macos.sh --assemble`) and signed by
`scripts/package-macos.sh`:

| Channel | Signature | Sandbox | Output | Card/token signing |
|---|---|---|---|---|
| Direct download (GitHub Releases) | Developer ID + hardened runtime, notarized and stapled | No | `build/Vitela-<version>-macos.zip` | Yes |
| Mac App Store | Apple Distribution + provisioning profile | Yes (required) | `build/Vitela-<version>-<build>-app-store.pkg` | No¹ |

¹ A PKCS#11 module is native code from the token vendor, loaded from outside
the app bundle. The App Sandbox and App Review guideline 2.5.2 do not allow
that, so the sandboxed build hides "Sign with Card or Token…". Certificate
files and Keychain identities still work.

## Without a paid account: the unsigned download

Until there is a membership, every release tag still ships a Mac build:
`release.yml` passes the version to `macos.yml`, which assembles the
universal bundle, tests it and uploads it; `publish-macos` attaches
`Vitela-<version>-macos-unsigned.zip` to the GitHub Release that
`publish-linux` created and appends opening instructions to its notes.

It carries only the ad-hoc signature, so Gatekeeper rejects it the first time
("Apple could not verify…"). The reader allows it once in **System Settings ▸
Privacy & Security ▸ Open Anyway** (Control-click ▸ Open on macOS 14 and
earlier); after that it opens like any app. Nothing about the app itself
differs from the Developer ID build: it is not sandboxed and keeps card or
token signing.

The same membership covers iOS. There is no free way to distribute the iOS
app: a free Apple ID only installs it on your own device from Xcode, for
seven days.

## With the Apple Developer Program

Everything below needs a **paid Apple Developer Program membership**
(99 USD/year). None of it can be done by an agent: it is the account holder's
identity, payment and private keys. Nothing here is ever committed.

### 1. Join the Apple Developer Program

1. Sign in at <https://developer.apple.com/account> with the Apple ID that
   will own the app (two-factor authentication must be on).
2. **Enroll** as an individual or as an organization. An organization needs a
   D-U-N-S number and its legal name is what the App Store shows as the
   seller. Approval usually takes from a few hours to two days.
3. Note your **Team ID** (Membership details): ten characters, such as
   `AB12CD34EF`. It appears in every certificate name below.

### 2. Certificates

Create them on the Mac that will sign, so the private key is generated in its
login keychain: **Xcode ▸ Settings ▸ Accounts ▸ (your team) ▸ Manage
Certificates ▸ +**. Only the Account Holder can create Developer ID ones.

| Certificate (Xcode name) | Used for | `package-macos.sh` variable |
|---|---|---|
| Developer ID Application | the direct download | `VITELA_DEVELOPER_ID` |
| Apple Distribution | the App Store app | `VITELA_APP_STORE_ID` |
| Mac Installer Distribution | the App Store `.pkg` | `VITELA_INSTALLER_ID` |

Check the exact names the scripts need:

```bash
security find-identity -v -p codesigning
security find-identity -v -p basic | grep Installer
```

Back up each one (Keychain Access ▸ right-click ▸ Export, as `.p12` with a
strong password). A lost Developer ID private key cannot be downloaded again.

### 3. Bundle identifier and App Store record

1. **Certificates, Identifiers & Profiles ▸ Identifiers ▸ +** ▸ App IDs ▸
   App ▸ platform macOS, *Explicit* bundle ID. The project uses
   `dev.vitela.viewer`; use a reverse domain you control instead if you prefer,
   and assemble with `VITELA_BUNDLE_ID=<that id>`. No capabilities are needed.
2. **Profiles ▸ +** ▸ Distribution ▸ **Mac App Store Connect**, that App ID,
   the Apple Distribution certificate. Download the `.provisionprofile`; it is
   `VITELA_PROFILE`.
3. In <https://appstoreconnect.apple.com> ▸ **Apps ▸ + ▸ New App**: platform
   macOS, name "Vitela", the bundle ID above, an SKU of your choice.
4. Fill in the listing before the first review: description, keywords,
   screenshots (1280×800 or larger), support URL, privacy policy URL (Vitela
   collects no data; say so), category Productivity, and age rating.
5. **Export compliance.** Vitela uses encryption (PDF passwords with AES,
   digital signatures). App Store Connect asks about it on each build; using
   standard algorithms qualifies for the mass-market exemption, but whether
   you must file the yearly self-classification report with the US BIS is
   your call as the publisher.

### 4. Credentials for notarization and upload

Create an **App Store Connect API key**: App Store Connect ▸ Users and
Access ▸ Integrations ▸ App Store Connect API ▸ Team Keys ▸ **+**, role
*Developer* (enough to upload and notarize). Download the `AuthKey_<KEYID>.p8`
**once** — Apple never shows it again — and note the Key ID and the Issuer ID
shown above the list.

| Variable | Value |
|---|---|
| `APPLE_API_KEY_PATH` | path to `AuthKey_<KEYID>.p8` |
| `APPLE_API_KEY_ID` | the Key ID |
| `APPLE_API_ISSUER` | the Issuer ID |

For notarization on your own Mac you can instead store them once in the
keychain and pass only the profile name:

```bash
xcrun notarytool store-credentials vitela-notary --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER"
```

then `VITELA_NOTARY_PROFILE=vitela-notary`. (An app-specific password from
<https://account.apple.com> also works with `store-credentials --apple-id`,
but the API key serves upload as well.)

### 5. Build, sign and ship

```bash
export PDFIUM_DYLIB="$PWD/core/pdf-render/vendor/pdfium/lib/libpdfium.dylib"
rustup target add aarch64-apple-darwin x86_64-apple-darwin
VITELA_UNIVERSAL=1 VITELA_VERSION=0.2.0 VITELA_BUILD_NUMBER=1 bash scripts/build-macos.sh --assemble
```

`VITELA_BUILD_NUMBER` must be higher on every App Store upload, even for the
same version.

Direct download:

```bash
VITELA_DEVELOPER_ID="Developer ID Application: <Name> (<TEAMID>)" VITELA_NOTARY_PROFILE=vitela-notary bash scripts/package-macos.sh developer-id
```

Mac App Store:

```bash
VITELA_APP_STORE_ID="Apple Distribution: <Name> (<TEAMID>)" VITELA_INSTALLER_ID="3rd Party Mac Developer Installer: <Name> (<TEAMID>)" VITELA_PROFILE=~/Downloads/Vitela_Mac_App_Store.provisionprofile bash scripts/package-macos.sh app-store
```

```bash
bash scripts/package-macos.sh upload build/Vitela-0.2.0-1-app-store.pkg
```

The upload appears in App Store Connect ▸ TestFlight after processing (about
15 minutes). Test it there through TestFlight for Mac, then attach the build
to a version and **Submit for Review**.

### Before the first submission: check the sandboxed build by hand

The automated gates cannot exercise powerbox grants. On a TestFlight install,
confirm:

- Open, Save As, Export Pages as Images, Split and Extract write where you
  pick; Finder's Open With and dropping a PDF on the window open it.
- Quit and reopen: Recent still shows its cards with previews and opens them.
- Print reaches a printer; Share offers its targets.
- Sign with Keychain Identity lists your identities (the system may ask to
  allow access to each key).
- File ▸ Sign with Card or Token… is absent.
