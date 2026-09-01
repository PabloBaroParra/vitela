# Ficha de batch B23 — Firma digital: orquestación y UI en Linux

> Documentado 2026-09-01, a pedido explícito del usuario, para encolarlo — arranca
> ahora, iterando fase por fase. Alcance acotado en la conversación que lo originó:
> **solo el shell Linux**, y **solo la firma criptográfica PKCS#7/PAdES** (identidad +
> CMS). El usuario declaró explícitamente no tener mucho conocimiento de certificados,
> así que las decisiones de UX de este documento están pensadas para alguien que no
> sabe la diferencia entre un `.pfx` y un token PKCS#11 hasta que se lo explican — ver
> "Decisiones de diseño" punto 2.

**Dependencias:** el núcleo criptográfico ya existe y está probado —
`core/pdf-sign` (44 tests), `core/pdf-sign-pfx` (7 tests, identidades desde archivo
`.p12`/`.pfx`) y `core/pdf-sign-pkcs11` (10 tests, identidades desde módulo PKCS#11 —
tarjeta inteligente, token USB). Lo que falta no es criptografía: es (1) una función de
orquestación de producción que encadene esas piezas con `pdf-save`, y (2) toda la UI de
Linux, que hoy es literalmente cero — el botón "Sign" del rail existe pero
`enabled: false`, y la pestaña "Fill & Sign" del panel de herramientas es un
placeholder explícito ("no feature behind them yet").

## Hecho clave del formato

Firmar un PDF no es "escribir una firma en un campo" — es un baile de cinco pasos ya
demostrado end-to-end en `tests/fixtures/gen-fixtures/src/signed.rs`
(`append_signed_revision`), la única referencia de producción que existe hoy de cómo
encadenar estas piezas:

1. `pdf_sign::SignatureFieldBuilder::new(name, page_object_id, rect).build()` — crea un
   campo `/FT /Sig` sin firmar, con un `/Contents` de tamaño fijo reservado
   (`DEFAULT_SIGNATURE_CAPACITY`, 16 KiB) y un `/ByteRange` con valores centinela.
2. `pdf_save::append_incremental_update(...)` — escribe ese campo como una revisión
   incremental, registrándolo en `/Annots` de la página y en `/AcroForm /Fields` +
   `SigFlags`.
3. `pdf_sign::prepare_signature_bytes(bytes, capacity)` — reemplaza los valores
   centinela del `/ByteRange` por los reales, ahora que el archivo tiene un tamaño
   fijo.
4. `pdf_sign::digest_byte_ranges(...)` — calcula el hash de todo el archivo **menos**
   el hueco donde va `/Contents` (eso es lo que se firma, no el archivo completo).
5. `CertificateSourcePort::list_identities()` → elegir una → `CmsSignedDataBuilder`
   construye el `SignedData` CMS/PKCS#7 → `pdf_sign::append_signature_bytes(...)`
   escribe el DER final en el hueco de `/Contents`.

Ninguna de estas cinco funciones es "la función de firmar" — son piezas. Hoy solo el
generador de fixtures las encadena, y es código de test, no reutilizable desde un
shell.

## Decisiones de diseño

1. **La orquestación vive dentro de `pdf-sign`, no en un crate nuevo.**
   `core/pdf-sign/Cargo.toml` ya tiene `pdf-document`/`pdf-manip`/`pdf-save` como
   `[dev-dependencies]`, con un comentario explícito: *"production pdf-sign stays an
   isolated leaf and must not pull the save/render dependency graph. Promote to
   `[dependencies]` only when signing orchestration code actually calls into
   pdf-save."* Este batch es exactamente ese momento — se promueven esas tres
   dependencias y se agrega un módulo nuevo (p. ej. `core/pdf-sign/src/orchestrate.rs`)
   con una función de producción que hace lo que `append_signed_revision` hace hoy en
   el generador de fixtures, con manejo de errores real (no `io::Error::other`) y sin
   asumir "página 1, rect invisible" como constantes hardcodeadas.
2. **PFX es el camino principal; PKCS#11 es "avanzado".** Un archivo `.pfx`/`.p12` con
   contraseña es lo que la mayoría de la gente sin conocimientos de certificados
   probablemente ya tiene (se lo dio un banco, una gestoría, la Seguridad Social,
   etc.) — es "elegí un archivo, escribí una contraseña", un patrón que cualquiera
   reconoce. PKCS#11 (tarjeta/token) requiere saber qué "módulo" (un `.so` del
   fabricante del lector) usar, algo que un usuario sin experiencia no va a poder
   nombrar. Por eso: el flujo de PFX va primero y es el que se explica en la UI con
   más detalle; PKCS#11 queda detrás de una opción "Usar tarjeta o token" que primero
   *intenta* una lista corta de rutas de módulo típicas de Linux (p. ej. las de
   `opensc-pkcs11.so` en las ubicaciones habituales de Debian/Ubuntu/Fedora) antes de
   pedirle al usuario que navegue a un archivo a mano — mismo espíritu que evitar
   pedirle a alguien sin conocimientos técnicos que escriba una ruta de memoria.
3. **La contraseña del `.pfx` nunca se guarda.** Mismo patrón que ya existe para abrir
   un PDF cifrado (`SaveBacking.password: Option<String>` se pide una vez por sesión,
   nunca se persiste a disco) — la contraseña de la identidad de firma vive solo en la
   llamada a `PfxCertificateSource::from_file`, nunca en `DocumentSession` ni en
   ningún lado que sobreviva esa llamada.
4. **Campo de firma invisible en v1, no una "firma dibujada" visible en la página.**
   El generador de fixtures ya firma con `rect: [0.0; 4]` (campo sin apariencia
   visual) y eso es un PDF firmado perfectamente válido — Acrobat/Preview lo
   reconocen igual. La fila "Sign PDF" del README menciona a futuro "Drawn
   signatures" (una firma manuscrita/visual sobre la página) como una feature
   *distinta* de la firma criptográfica; este batch no la toca — ver "Fuera de
   scope".
5. **El widget de firma se gatea igual que crear un campo de formulario.** Firmar
   agrega un campo `/FT /Sig` nuevo a `/AcroForm` y `/Annots` — estructuralmente es
   "crear un campo", no "llenar uno existente", así que usa el mismo criterio que
   `forms::command::structural_edit_refusal` (bits ISO 32000-1 Tabla 22 6 *y* 4:
   anotar/formularios, y modificar contenido). No hay un bit de permiso dedicado a
   "firmar" en el estándar — esto es un criterio razonable, no un mandato del spec, y
   queda abierto a que el usuario lo corrija si prefiere otro.
6. **Re-guardar un documento ya firmado no es tarea de este batch — ya existe.**
   `apps/linux-gtk/src/app/document.rs` ya maneja
   `pdf_save::SignatureAcknowledgement`/`SaveError::SignaturesWouldBeInvalidated`: si
   editás y guardás un PDF firmado, ya te avisa que eso invalida la firma y te deja
   decidir. Este batch solo agrega *crear* la primera firma.
7. **Elegir el algoritmo de firma es automático, no una pregunta al usuario.** Cada
   `SigningIdentity` ya declara sus `supported_algorithms` (RSA-PKCS1v15 o ECDSA, con
   SHA-256/384/512) — la orquestación elige el primero que la identidad soporte en
   vez de mostrarle a alguien sin conocimientos de cripto un selector de "RSA vs
   ECDSA" que no va a saber interpretar.
8. **Solo Linux.** Windows/macOS/iOS/Android no se tocan en este batch — cada shell
   tiene su propio mecanismo nativo de almacén de certificados (CryptoAPI/Keychain)
   que amerita su propio adaptador y su propia ficha más adelante.

## Tareas

### Fase 1 — Orquestación de producción (`core/pdf-sign`)
- [x] T-177 Promover `pdf-document`/`pdf-manip`/`pdf-save` de `[dev-dependencies]` a
      `[dependencies]` en `core/pdf-sign/Cargo.toml`.
- [x] T-178 `orchestrate.rs`: `pub fn sign_document(bytes, document_password,
      page_number, field_name, source: &dyn CertificateSourcePort, identity_id) ->
      Result<Vec<u8>, SignError>` que encadena los cinco pasos de "Hecho clave del
      formato" contra bytes de PDF reales, con página elegible por el llamador
      (no hardcodeada a 1) y errores tipados nuevos en `SignError`
      (`PageNotFound`, `NoSupportedAlgorithm`, `DocumentOpen`, `IncrementalWrite`)
      en vez de `io::Error`. [SignOrchestration]
- [x] T-179 Tests (4, con un `CertificateSourcePort` falso in-process — sin
      dependencia de `pdf-sign-pfx` real para evitar un ciclo de dev-dependencies
      con ese crate, que sí la depende a ella): firmar un documento sin `/AcroForm`
      previo, firmar dos veces en secuencia (la segunda no pisa la primera en
      `/Fields`), página inexistente rechazada, identidad desconocida rechazada.
      [SignOrchestration]

### Fase 2 — Linux: identidad desde archivo `.pfx`/`.p12`
- [x] T-180 Diálogo de archivo (`GtkFileDialog`) filtrado a `.pfx`/`.p12` + prompt de
      contraseña — mismo patrón visual que el prompt de contraseña ya existente para
      abrir un PDF cifrado. [LinuxSignUI]
- [x] T-181 `PfxCertificateSource::from_file(path, password)` → si falla (contraseña
      incorrecta, archivo corrupto), mensaje de error claro, sin dejar el diálogo en
      un estado ambiguo. Si funciona, pasa a la Fase 4 con las identidades listadas.
      [LinuxSignUI]
      - Entregado como un botón real "Choose signing certificate (.pfx)…" en la
        pestaña "Fill & Sign" (`apps/linux-gtk/src/app/sign/mod.rs`) — necesario
        para que el código no quede muerto bajo `-D warnings` antes de que la Fase 4
        exista. Reporta las identidades encontradas por la barra de estado; no
        persiste nada todavía (ni el `PfxCertificateSource` cargado ni la lista de
        `SigningIdentity`) — la Fase 4 diseñará su propio estado para eso, informada
        también por lo que necesite la Fase 3 (PKCS#11).

### Fase 3 — Linux: identidad desde tarjeta/token PKCS#11
- [x] T-182 Lista corta de rutas de módulo PKCS#11 típicas de Linux, probadas en
      orden (decisión 2) antes de ofrecer "elegir archivo `.so` manualmente" como
      alternativa. [LinuxSignUI]
- [x] T-183 Prompt de PIN (no "contraseña" — es la terminología que el propio token
      usa) + `Pkcs11CertificateSource::load(module_path, pin)`. Mismo manejo de error
      que T-181: un PIN bloqueado o un módulo que no carga tiene que decir por qué,
      no fallar en silencio. [LinuxSignUI]
      - Entregado como el botón "Use card or token…" en la pestaña "Fill & Sign"
        (`apps/linux-gtk/src/app/sign/mod.rs`), junto al de PFX. `find_pkcs11_module`
        prueba las rutas típicas de `opensc-pkcs11.so` (Debian/Ubuntu multiarch,
        Fedora/RHEL/openSUSE `lib64`, y variantes sin subcarpeta `pkcs11`) por
        existencia de archivo; si ninguna existe, abre un `GtkFileDialog` filtrado a
        `*.so`. A diferencia del PFX, `Pkcs11CertificateSource::load` no valida el PIN
        — un PIN incorrecto degrada silenciosamente a listar solo los certificados
        públicos del token (ver `pin_attempt_is_safe` en el crate), así que un PIN
        malo y un token vacío llegan aquí como "cero identidades"; el prompt trata
        ambos casos igual, dejando reintentar el PIN sin tener que re-elegir el
        módulo. Un módulo que no carga (`Pkcs11AdapterError::Module`) sí es un error
        real y cierra el diálogo con el mensaje de la librería. No persiste nada
        todavía, mismo punto que T-181 — la Fase 4 diseñará el estado compartido para
        ambas fuentes.

### Fase 4 — Selector de identidad y acción de firmar
- [x] T-184 Panel/diálogo que lista `SigningIdentity::display_name` de las
      identidades encontradas (de cualquiera de las dos fases anteriores), el
      usuario elige una y confirma. [LinuxSignUI]
      - Entregado como `open_identity_picker` (`apps/linux-gtk/src/app/sign/mod.rs`):
        se abre automáticamente en cuanto la Fase 2 o la Fase 3 desbloquea al menos
        una identidad — una lista de `CheckButton` agrupados como radio (mismo
        patrón que `forms::fill::build_radio_group`), la primera preseleccionada.
- [x] T-185 Al confirmar: llama a `sign_document` (T-178), sobrescribe/guarda el
      resultado, recarga el documento como cualquier otro cambio estructural (mismo
      ciclo guardar→reabrir que ya usa el resto del shell), y reporta éxito/error en
      la barra de estado. [LinuxSignUI]
      - Entregado como `begin_sign_from_picker` (gate + extracción de estado) →
        `document::begin_sign` (`apps/linux-gtk/src/app/document.rs`), gemelo de
        `show_save_chooser_then`/`save_current_to`/`spawn_save`: este shell no
        recuerda "el archivo actual" para sobrescribirlo implícitamente — ni
        siquiera un Save ordinario lo hace — así que firmar también pregunta el
        destino con un diálogo, corre `pdf_sign::sign_document` en un hilo de
        fondo, valida el resultado con pdfium, escribe atómicamente y reabre.
        Dos decisiones no cubiertas por la ficha: (1) el nombre del campo se
        genera contando cuántos `/FT /Sig` ya tiene la base (`Signature_N+1`) en
        vez de un literal fijo, porque T-179 ya prueba que `sign_document` permite
        refirmar un documento ya firmado y un nombre repetido colisionaría en
        `/AcroForm /Fields`; (2) firmar se rechaza (con un error inline en el
        selector, sin cerrar el diálogo) si `session.unsaved_to_disk` está en
        `true` — `sign_document` firma `SaveBacking::original_bytes` sin pasar por
        `document_model`/`EditLog`, así que firmar con ediciones pendientes las
        descartaría en silencio al reabrir.

### Fase 5 — Wiring final
- [ ] T-186 Habilita el botón "Sign" del rail (`shell.rs`) y la pestaña "Fill & Sign"
      deja de ser un placeholder — conecta al flujo de las Fases 2-4, gateado por el
      criterio de la decisión 5. [LinuxSignUI]

## Criterios de aceptación

- Firmar un PDF sin firma previa con un `.pfx` produce un archivo que Acrobat/
  Preview/`exiftool`/`openssl cms -verify` reconocen como firmado y con la firma
  válida.
- Firmar con una identidad PKCS#11 (probado contra SoftHSM o un token real
  disponible) produce el mismo resultado verificable.
- Una contraseña de `.pfx` o PIN de token incorrectos muestran un error claro y no
  dejan el documento en un estado a medio firmar.
- Volver a guardar un documento recién firmado sigue pasando por el flujo de
  `SignatureAcknowledgement` ya existente, sin cambios de comportamiento ahí.
- Ninguna contraseña, PIN, o material de clave privada queda logueado ni persistido
  más allá de la llamada que lo necesita.
- El botón "Sign" y la pestaña "Fill & Sign" quedan sensibles/funcionales solo cuando
  el documento lo permite (mismo criterio que cualquier otro control estructural del
  shell).

## Fuera de scope (v1)

Firma manuscrita/visual ("Drawn signatures" del README) — es una feature de
apariencia sobre la página, ortogonal a la firma criptográfica que cubre este batch ·
verificación de firmas existentes en la UI (quién firmó, si la cadena de confianza es
válida) — `pdf-sign` ya hace verificación *estructural*, pero mostrarla en pantalla es
trabajo de UI aparte que nadie pidió todavía · timestamping (TSA) y OCSP — el propio
README ya declara esta feature "offline, no TSA/OCSP" · Windows/macOS/iOS/Android —
cada uno con su propio almacén nativo de certificados, ficha aparte · firmas
múltiples simultáneas de PKCS#11 más allá de listarlas todas y dejar elegir una.

## Orden de ejecución

Fase 1 primero y sola — es la única con TDD estricto sobre código nuevo de producción,
y las Fases 2-4 no tienen nada que llamar hasta que exista. Fase 2 (PFX) antes que
Fase 3 (PKCS#11) por la decisión 2 — es el camino que más gente puede probar sin
hardware especial. Fase 4 depende de que exista al menos una de las dos anteriores
(puede arrancar en paralelo con la que falte). Fase 5 al final, cuando 1-4 ya
funcionan de punta a punta.
