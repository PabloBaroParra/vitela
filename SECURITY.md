# Security Policy

## Reporting a vulnerability

**Do not open a public issue for a security report.**

Use GitHub's private vulnerability reporting: go to the
[Security tab](https://github.com/PabloBaroParra/vitela/security/advisories/new)
and open a draft advisory. It is private between you and the maintainers, and
it lets us prepare a fix and an advisory together.

Please include:

- what an attacker can achieve, not only what is wrong
- the steps or a minimal PDF that reproduces it (see the note below)
- the affected crate or shell, and the version or commit
- your platform, and the PDFium build in use if rendering is involved

**Acknowledgement within 72 hours**, an assessment within 7 days, and we will
keep you updated until it is resolved. If you want credit in the advisory, say
so — it is yours by default unless you ask to stay anonymous.

Please give us a reasonable window to ship a fix before disclosing publicly. We
will not take legal action against good-faith research that respects that.

### Sending a proof-of-concept file

A malicious PDF is often the whole report. Attach it to the draft advisory
rather than emailing it, and note in the report if it contains anything you
would rather we not retain. If the file contains real personal data, redact it
or synthesise an equivalent — we do not want your documents.

## Supported versions

Vitela is pre-release and has no published releases yet. Only `main` is
supported; fixes land there. Once versions are published, this section will list
the supported range.

## What we consider a vulnerability

Vitela makes promises a user cannot verify on their own, so breaking one of them
is a security issue even when nothing crashes:

| Area | Why it matters |
| --- | --- |
| **Any outbound network traffic** | Vitela guarantees offline-first operation with zero telemetry. A request to any host — including a dependency's — is a vulnerability, not a bug. |
| **Encryption silently dropped** | `pdf-save` re-applies encryption on save. A path that writes a previously encrypted document in the clear exposes the user without telling them. |
| **Signature invalidation or forgery** | `pdf-sign` and `pdf-sign-pkcs11` produce PKCS#7/PAdES signatures. Anything that forges one, or that quietly invalidates an existing signature while reporting success, is in scope. |
| **Password handling** | `pdf-manip` decrypts on open (RC4-128 / AES-128). Leaking a password into logs, memory dumps or error text is in scope. |
| **PKCS#11 token handling** | Mishandling a hardware token, its PIN, or its session state. |
| **Memory safety at the boundaries** | Parsing untrusted PDFs, the `pdf-ffi` UniFFI surface, and the RGBA buffers handed to platform image APIs — a short buffer read as a full one is an out-of-bounds read, not a blank page. |
| **Path handling on save/export** | Writing outside the location the user chose. |

### Out of scope

- Vulnerabilities in PDFium itself — report those to
  [the upstream project](https://pdfium.googlesource.com/pdfium/); we will pick
  up the fixed build. Tell us anyway if Vitela's usage makes it reachable.
- Findings that require an attacker who already has code execution or full disk
  access on the user's machine.
- Missing hardening headers, or automated-scanner output with no demonstrated
  impact.
- The absence of signing, notarization and distribution on the Apple platforms.
  This is known and documented in the README, not a finding.

## Security-relevant design

Some deliberate decisions that are useful context for a report:

- **No network stack by design.** CI runs the test suite inside a network
  namespace with no routes, so a new outbound call fails the build rather than
  shipping.
- **Encryption preservation is a default, not an option.** Removing it requires
  an explicit action.
- **Incremental saves** keep existing signatures valid; a full rewrite is used
  only when the document requires it.
- **Dependencies are audited in CI** with `cargo-deny` (advisories, bans,
  sources) and updated through Dependabot.
- **Prebuilt binaries are pinned.** PDFium comes from a pinned
  `bblanchon/pdfium-binaries` release, never a floating "latest".
