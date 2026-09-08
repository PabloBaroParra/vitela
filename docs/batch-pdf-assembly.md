# Checklist de combinación y organización de PDFs en Linux

Este documento sigue la implementación de la importación de varios PDFs en un
documento abierto y su organización por documentos completos o por páginas
individuales. La primera entrega corresponde exclusivamente al shell GTK4 de
Linux; el comportamiento reutilizable debe permanecer en el núcleo Rust.

## Cómo actualizar este checklist

- Una tarea solo pasa a `[x]` cuando su implementación y sus pruebas están
  completas.
- Cada tarea terminada debe añadir una nota breve con fecha, archivos relevantes
  y evidencia de verificación.
- Los trabajos parciales permanecen en `[ ]` y se documentan como parciales.
- Los gates no ejecutados deben figurar como no verificados; nunca se asumen
  correctos por haber pasado otros gates.
- Las decisiones que cambien alcance, contratos o preservación de datos deben
  quedar registradas antes de continuar la implementación.

## Estado

| Área | Estado |
|------|--------|
| Modelo y operaciones | En progreso |
| Importación PDF | Pendiente |
| Guardado y renderizado | Pendiente |
| Integración Linux | Pendiente |
| Vista por documentos | Pendiente |
| Vista por páginas | Pendiente |
| Animación | Pendiente |
| Pruebas y gates | Pendiente |

## Decisiones cerradas

- [x] Limitar la primera implementación al shell GTK4 de Linux.
- [x] Permitir elegir entre organizar documentos completos y páginas.
- [x] Usar el modo por documentos como vista inicial.
- [x] Mantener exactamente el orden de las páginas al cambiar de vista.
- [x] Representar como bloques separados los tramos no contiguos de un mismo
  PDF.
- [x] Evitar que un cambio de vista modifique el documento o cree historial.
- [x] Preservar contenido PDF real; no rasterizar páginas importadas.

## Límites de responsabilidad

- `pdf-document` posee la identidad, procedencia y orden lógico de las páginas,
  además de los comandos reversibles.
- `pdf-manip` posee la copia y remapeo de objetos PDF entre documentos.
- `pdf-save` materializa el orden lógico final sobre el documento PDF.
- El shell Linux posee selección de archivos, diálogos, estado visual,
  animación y cableado de eventos.
- La UI no debe manipular directamente árboles de objetos PDF.
- El núcleo no debe depender de GTK, rutas seleccionadas por la UI ni widgets.

## 1. Modelo y operaciones

- [x] Añadir procedencia explícita para páginas del documento base, páginas en
  blanco y páginas importadas.
- [x] Mantener `PageId` como identidad estable e independiente del índice
  visual o de renderizado.
- [ ] Asignar identificadores únicos a cada PDF importado.
- [ ] Registrar cada fuente importada una sola vez en el respaldo de la sesión.
- [ ] Añadir un comando atómico para insertar todas las páginas seleccionadas
  de un PDF.
- [ ] Hacer que deshacer y rehacer una importación requiera un único paso.
- [ ] Añadir un comando atómico para mover un tramo contiguo de páginas.
- [ ] Hacer que mover un bloque requiera un único paso de deshacer.
- [ ] Añadir validación para impedir identificadores duplicados, rangos inválidos
  y órdenes que no sean permutaciones del estado actual.
- [ ] Centralizar en el núcleo la clasificación de comandos estructurales de
  página.
- [ ] Probar aplicación, inversión, deshacer y rehacer de los nuevos comandos.

### Progreso del modelo

- 2026-09-08: `pdf-document` incorpora `PageOrigin::{Base, Blank, Imported}` e
  `ImportedDocumentId`. La procedencia conserva índices de fuente separados de
  `PageId`; los bytes y objetos PDF siguen fuera del modelo puro.
- 2026-09-08: `pdf-save` registra como `Base` las páginas abiertas y rechaza
  páginas `Imported` hasta que exista el injerto real, evitando convertirlas
  silenciosamente en páginas en blanco.
- Verificación: `cargo fmt --all -- --check`; `cargo test -p pdf-document`
  (86 aprobadas); `cargo test -p pdf-save` (119 aprobadas, 4 ignoradas);
  `cargo clippy -p pdf-document -p pdf-save --all-targets -- -D warnings`.

## 2. Derivación de bloques

- [ ] Derivar los bloques recorriendo `Document.pages` de izquierda a derecha.
- [ ] Agrupar únicamente páginas contiguas procedentes del mismo PDF.
- [ ] Mantener el orden interno exacto de cada tramo.
- [ ] Crear un bloque nuevo cuando reaparezca una fuente después de páginas de
  otra fuente.
- [ ] Identificar cada bloque mediante datos estables, no mediante su posición
  visual.
- [ ] Etiquetar tramos divididos como `Part 1`, `Part 2`, etc.
- [ ] Recalcular los bloques después de importar, mover, borrar, deshacer o
  rehacer.
- [ ] Probar secuencias intercaladas como `A1, A2, B1, A3, B2`.
- [ ] Verificar que derivar bloques nunca muta el documento.

## 3. Importación PDF

- [ ] Implementar una operación de injerto de páginas en `pdf-manip`.
- [ ] Copiar y remapear el grafo de objetos alcanzable desde cada página.
- [ ] Materializar los atributos heredados necesarios antes de cambiar el
  padre de una página.
- [ ] Conservar streams de contenido, fuentes, imágenes, XObjects, espacios de
  color y patrones.
- [ ] Conservar cajas de página, rotación y recursos heredados.
- [ ] Conservar enlaces, acciones y anotaciones asociados a las páginas.
- [ ] Evitar colisiones entre identificadores de objetos de distintos PDFs.
- [ ] Excluir de la copia el catálogo, trailer, cifrado y metadatos globales de
  la fuente cuando no deban gobernar el documento resultante.
- [ ] Devolver el mapa de objetos necesario para resolver las páginas
  importadas después del injerto.
- [ ] Hacer que una entrada malformada falle sin modificar parcialmente el
  destino.

## 4. Estructuras de documento

- [ ] Definir y probar la política para formularios AcroForm importados.
- [ ] Resolver colisiones de nombres de campos sin fusionarlos silenciosamente.
- [ ] Conservar widgets y apariencias cuando se acepten formularios.
- [ ] Definir y probar la política para marcadores y destinos con nombre.
- [ ] Remapear destinos que apunten a páginas importadas.
- [ ] Detectar destinos que apunten a páginas no importadas.
- [ ] Definir la política para capas opcionales y estructura etiquetada.
- [ ] Rechazar con un mensaje claro cualquier estructura todavía no soportada
  que pudiera perder información.
- [ ] No ofrecer una importación aparentemente correcta si existe pérdida de
  datos conocida.

## 5. Seguridad y firmas

- [ ] Añadir una comprobación específica del permiso PDF de ensamblado de
  documentos.
- [ ] Comprobar permisos tanto en el documento principal como en cada fuente.
- [ ] Solicitar de forma independiente la contraseña de cada PDF protegido.
- [ ] Mantener las credenciales fuera del modelo de dominio, logs y mensajes de
  error.
- [ ] Verificar antes de editar que un documento principal cifrado puede
  reescribirse correctamente.
- [ ] Mantener la política de cifrado del documento principal al guardar.
- [ ] Detectar firmas en el documento principal y en las fuentes activas.
- [ ] Explicar que una combinación o reordenación invalida criptográficamente
  las firmas existentes.
- [ ] Exigir confirmación explícita antes de guardar un resultado que invalide
  firmas.
- [ ] Conservar objetos y apariencias de firma solo cuando la política definida
  lo permita.

## 6. Guardado y resolución de páginas

- [ ] Extender `pdf-save` para distinguir páginas en blanco de páginas
  importadas.
- [ ] Reconstruir el PDF siguiendo exactamente el orden de `Document.pages`.
- [ ] Mantener el documento base original inmutable durante la edición.
- [ ] Resolver cada página importada mediante su fuente y número de página.
- [ ] Devolver un mapa final de `PageId` a objeto PDF después de materializar.
- [ ] Resolver explícitamente `PageId` a índice de renderizado actual.
- [ ] Eliminar los usos que asumen que `PageId.0` es un índice de PDFium.
- [ ] Leer anotaciones existentes desde el documento materializado para no
  borrar anotaciones importadas al añadir otras nuevas.
- [ ] Permitir edición de contenido sobre páginas importadas usando el respaldo
  materializado correcto.
- [ ] Validar el resultado con PDFium antes de instalar la previsualización.
- [ ] Probar guardar, cerrar y reabrir después de importar, mover y borrar.

## 7. Actualización de la sesión Linux

- [ ] Generalizar el ciclo de guardar en memoria y reabrir usado por edición de
  contenido.
- [ ] Mantener el historial completo durante la actualización de la
  previsualización.
- [ ] Mantener el estado de cambios sin guardar.
- [ ] Mantener selecciones y contadores de identificadores que sigan siendo
  válidos.
- [ ] Invalidar selecciones y cachés que apunten a páginas eliminadas.
- [ ] Instalar conjuntamente el nuevo handle de PDFium y su mapa de páginas.
- [ ] Evitar que un resultado asíncrono antiguo reemplace una sesión más nueva.
- [ ] Conservar la sesión anterior intacta si falla la materialización o la
  reapertura.

## 8. Selección e importación en Linux

- [ ] Añadir `Add PDFs` a la cabecera de Organizar.
- [ ] Permitir selección múltiple mediante `FileDialog::open_multiple`.
- [ ] Respetar el orden devuelto por la selección de archivos.
- [ ] Ejecutar lectura, validación e importación fuera del hilo principal.
- [ ] Mostrar progreso para operaciones perceptibles.
- [ ] Permitir cancelar sin modificar documento, historial ni estado sucio.
- [ ] Mostrar errores por archivo con una explicación accionable.
- [ ] Añadir inicialmente cada PDF como un bloque al final del documento.
- [ ] Actualizar controles de guardado, deshacer y rehacer tras importar.

## 9. Vista por documentos

- [ ] Añadir el selector visual `Documents | Pages`.
- [ ] Abrir Organizar con `Documents` seleccionado.
- [ ] Mostrar cada tramo contiguo como una tarjeta de documento.
- [ ] Incluir portada apilada, nombre, número de páginas y rango actual.
- [ ] Mantener una jerarquía visual limpia y coherente con la paleta existente.
- [ ] Permitir arrastrar una tarjeta para mover todo el tramo.
- [ ] Resolver inserciones antes, después y al final de la lista.
- [ ] Mostrar claramente el destino durante el arrastre.
- [ ] Permitir eliminar un bloque mediante una única acción reversible.
- [ ] Añadir nombres accesibles y ayudas de teclado para mover y eliminar.
- [ ] Evitar depender exclusivamente de arrastrar y soltar.

## 10. Vista por páginas

- [ ] Reutilizar la cuadrícula individual existente sin duplicar decisiones de
  negocio.
- [ ] Permitir mezclar páginas de diferentes PDFs.
- [ ] Mantener reordenación y eliminación como operaciones reversibles.
- [ ] Transferir identidades estables durante el arrastre, no índices
  capturados.
- [ ] Resolver inserciones en huecos y después de la última tarjeta.
- [ ] Actualizar números de página después de cada operación.
- [ ] Mostrar la procedencia de una página sin sobrecargar visualmente la
  tarjeta.
- [ ] Verificar que volver a `Documents` conserva exactamente el orden actual.

## 11. Animación y rendimiento

- [ ] Construir vistas independientes dentro de un `GtkStack`.
- [ ] Evitar reparentar widgets retirados de un `FlowBox`.
- [ ] Animar el paso de documentos a páginas con opacidad, escala y
  desplazamiento.
- [ ] Escalonar la aparición de páginas para comunicar que salen de sus
  bloques.
- [ ] Limitar la duración total de la secuencia en documentos grandes.
- [ ] Animar solo elementos visibles cuando hacerlo completo sea costoso.
- [ ] Respetar la configuración global de animaciones de GTK.
- [ ] Cambiar inmediatamente de vista cuando las animaciones estén
  desactivadas.
- [ ] Cachear miniaturas por documento, página, tamaño y factor de escala.
- [ ] Invalidar resultados de renderizado obsoletos mediante una generación.
- [ ] Evitar renderizar nuevamente todas las miniaturas al cambiar de vista.
- [ ] Medir importación, primer render y cambio de modo con documentos grandes.

## 12. Pruebas del núcleo

- [ ] Probar que importar un PDF es un único paso de historial.
- [ ] Probar que deshacer y rehacer restaura orden y procedencia.
- [ ] Probar que mover un bloque es un único paso de historial.
- [ ] Probar importación de texto, imágenes y recursos anidados.
- [ ] Probar páginas con atributos heredados y árboles de páginas anidados.
- [ ] Probar colisiones de identificadores entre documentos.
- [ ] Probar enlaces entre páginas importadas.
- [ ] Probar anotaciones y sus streams de apariencia.
- [ ] Probar formularios, campos homónimos y widgets.
- [ ] Probar marcadores y destinos con nombre según la política acordada.
- [ ] Probar fuentes cifradas y permisos insuficientes.
- [ ] Probar documentos principales cifrados con credenciales completas e
  incompletas.
- [ ] Probar advertencias de firma para destino y fuentes.
- [ ] Probar que los errores no dejan mutaciones parciales.
- [ ] Mantener determinismo con reloj e identificadores inyectados.

## 13. Pruebas GTK4

- [ ] Probar disponibilidad y estado del botón de importación.
- [ ] Probar selección múltiple y cancelación.
- [ ] Probar el selector `Documents | Pages`.
- [ ] Probar que el selector no crea comandos ni marca cambios.
- [ ] Probar tarjetas, nombres, recuentos, rangos y etiquetas de partes.
- [ ] Probar arrastre de bloques como una operación atómica.
- [ ] Probar mezcla de páginas y reconstrucción de tramos.
- [ ] Probar que el orden permanece intacto al volver a documentos.
- [ ] Probar destinos de arrastre en huecos y al final.
- [ ] Probar alternativas de teclado y etiquetas accesibles.
- [ ] Probar tipo y duración configurada de la transición sin depender de
  temporizadores reales.
- [ ] Probar el comportamiento con animaciones desactivadas.
- [ ] Probar que resultados asíncronos obsoletos se descartan.
- [ ] Probar que una importación fallida conserva la sesión anterior.

## 14. Gates de verificación

- [ ] Ejecutar `cargo fmt --all -- --check`.
- [ ] Ejecutar `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- [ ] Ejecutar `cargo build --workspace --locked`.
- [ ] Ejecutar `cargo test --workspace --locked -- --skip gtk_ui_`.
- [ ] Ejecutar la suite GTK4 bajo Xvfb en Linux.
- [ ] Ejecutar el empaquetado y smoke test de Linux.
- [ ] Ejecutar `python3 scripts/check_maintainability.py`.
- [ ] Registrar comandos, resultados y gates no disponibles en la entrega.

## Criterios de cierre

- [ ] Se pueden añadir varios PDFs al documento abierto desde Linux.
- [ ] Se pueden ordenar como documentos completos o como páginas individuales.
- [ ] Los cambios entre vistas nunca alteran el orden.
- [ ] Los tramos separados del mismo PDF aparecen como bloques separados.
- [ ] El contenido importado conserva su naturaleza PDF y no se rasteriza.
- [ ] Importaciones y movimientos de bloque tienen deshacer y rehacer atómicos.
- [ ] Guardar y reabrir conserva contenido, orden y estructuras soportadas.
- [ ] Los casos no soportados se rechazan antes de perder información.
- [ ] La interfaz permanece fluida con documentos grandes.
- [ ] Todos los gates disponibles están ejecutados y documentados.

## Fuera de alcance inicial

- [ ] Implementación de la interfaz equivalente en Windows.
- [ ] Implementación de la interfaz equivalente en macOS.
- [ ] Implementación de la interfaz equivalente en Android.
- [ ] Implementación de la interfaz equivalente en iOS.
- [ ] Persistencia de la procedencia original mediante metadatos personalizados
  después de cerrar y volver a abrir el PDF resultante.
