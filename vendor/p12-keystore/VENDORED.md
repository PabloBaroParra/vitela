# Vendored: p12-keystore

- **Upstream**: https://github.com/ancwrd1/p12-keystore.git
- **Commit**: `59114f5048a1487866be55dc8cd2608cf4573bfa` — "Re-export PrivateKey struct. Fixes #10." Equivale al contenido publicado como 0.3.1 en crates.io (el release ya incluye el re-export de `PrivateKey`/`PrivateKeyChain`).
- **Parches locales** (razón única del vendoring — no existen upstream):
  1. `src/keystore.rs` (~línea 135): cycle-guard con `BTreeSet` en el recorrido de emisores de `KeyStore::from_pkcs12`. Evita loop infinito / agotamiento de memoria ante cadenas de certificados con emisores cíclicos (finding R4-001 de review-06c623869d708fca).
  2. `src/codec.rs`: límites a parámetros controlados por el atacante — iteraciones del MAC en `verify_mac` (máx. 1.000.000, finding R4-001 de review-t077b-scope-recovery-20260715-01), iteraciones PBES1 y PBES2/PBKDF2 en `decrypt` (máx. 1.000.000), y para scrypt el coste efectivo de memoria `128·N·r` (máx. 1 GiB, con `checked_mul` anti-overflow) y la paralelización `p` (máx. 16). Nota: acotar solo `N` es insuficiente porque `r` y `p` también salen del DER del contenedor. Evita DoS de CPU/memoria con contenedores PFX maliciosos.
  3. `Cargo.toml`: tabla `[workspace]` vacía para poder ejecutar la suite de tests del crate de forma autónoma dentro del árbol del workspace.
  4. Lectura de PKCS#12 exportados por OpenSSL 1.x / LibreSSL: `Cargo.toml` activa las features `sha1-insecure` y `3des` de `pkcs5` (PBES2 con PBKDF2-HMAC-SHA1 — el PRF por omisión cuando el contenedor no lo nombra — y PBES2 con DES-EDE3-CBC; solo se usan para descifrar, el writer sigue emitiendo PBKDF2-SHA256 + AES-256). `src/codec.rs` (`decrypt`) devuelve `Error::UnsupportedEncryptionScheme` también cuando `pkcs5` no reconoce el esquema o el PRF de PBES2, en lugar de `DerError`/`Pkcs5Error`, para que el llamador pueda distinguir "cifrado no soportado" de "contraseña incorrecta" (`pdf-sign-pfx` lo muestra al usuario).
- **Consumo**: vía `[patch.crates-io]` en el Cargo.toml raíz.
- **Al re-vendorear desde upstream**: verificar si los parches 1, 2 y 4 ya fueron integrados; si no, re-aplicarlos antes de actualizar. Hay PRs upstream propuestos con ambos parches.
