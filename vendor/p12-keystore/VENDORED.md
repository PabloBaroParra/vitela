# Vendored: p12-keystore

- **Upstream**: https://github.com/ancwrd1/p12-keystore.git
- **Commit**: `59114f5048a1487866be55dc8cd2608cf4573bfa` — "Re-export PrivateKey struct. Fixes #10." Equivale al contenido publicado como 0.3.1 en crates.io (el release ya incluye el re-export de `PrivateKey`/`PrivateKeyChain`).
- **Parches locales** (razón única del vendoring — no existen upstream):
  1. `src/keystore.rs` (~línea 135): cycle-guard con `BTreeSet` en el recorrido de emisores de `KeyStore::from_pkcs12`. Evita loop infinito / agotamiento de memoria ante cadenas de certificados con emisores cíclicos (finding R4-001 de review-06c623869d708fca).
  2. `src/codec.rs`: límites a parámetros KDF controlados por el atacante en `decrypt` — iteraciones PBES1 y PBES2/PBKDF2 (máx. 1.000.000) y coste scrypt `N` (máx. 2^20). Evita DoS de CPU/memoria con contenedores PFX maliciosos (lineage review-t077b-scope-recovery-20260715-01).
  3. `Cargo.toml`: tabla `[workspace]` vacía para poder ejecutar la suite de tests del crate de forma autónoma dentro del árbol del workspace.
- **Consumo**: vía `[patch.crates-io]` en el Cargo.toml raíz.
- **Al re-vendorear desde upstream**: verificar si los parches 1 y 2 ya fueron integrados; si no, re-aplicarlos antes de actualizar. Hay PRs upstream propuestos con ambos parches.
