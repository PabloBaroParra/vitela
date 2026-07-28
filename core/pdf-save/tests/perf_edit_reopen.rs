//! Perf harness (pre-work para B8/T-047): qué cuesta UNA anotación desde que
//! el usuario la suelta hasta que la ve pintada.
//!
//! Hoy los cuatro shells son visores: abren un `DocumentHandle` de pdfium y lo
//! tratan como inmutable. pdfium no sabe nada del `EditLog` — para que un edit
//! se vea rasterizado hay que serializar el documento (`save_document`),
//! reabrirlo (`open_document_from_bytes`) y volver a renderizar la página
//! visible. Ése es el WARNING de UX que la ficha de B8 dejó explícitamente
//! parqueado "para decidir con datos de shell real".
//!
//! Este harness produce esos datos. Mide, por tamaño de documento, las patas
//! del ciclo por separado:
//!
//! - `baseline` — `render_page` sola sobre el handle vigente: lo que ya cuesta
//!   hoy un repintado cualquiera (scroll, cambio de zoom).
//! - `model load` — `open_document` + `document_from_lopdf`: la estructura que
//!   un shell que EDITA tiene que sostener además del handle de pdfium, y que
//!   hoy ningún shell carga.
//! - `input`, `save`, `reopen`, `close` — el sobrecosto que agrega hacer el
//!   round-trip en cada edit.
//!
//! El número que decide el diseño es `overhead / baseline`:
//!
//! - cerca de 1 o menos → el round-trip por edit es gratis frente a un
//!   repintado; los shells pueden re-renderizar en cada edit sin más.
//! - bastante mayor que 1 → hay que dibujar el edit como overlay en el canvas
//!   del shell y round-trippear sólo al guardar — que es exactamente lo que
//!   T-142 ya eligió para los campos de formulario ("sin re-render pdfium; /V
//!   + /AP se escriben al guardar").
//!
//! Deliberadamente NO afirma un presupuesto en milisegundos: no hay ninguno en
//! spec para un edit (el 1.5s de "Large-File Performance" es para la apertura),
//! e inventar uno acá sería fijar la decisión antes de medirla. Las
//! aserciones cubren sólo lo que tiene que valer sí o sí — que los edits
//! aterrizan y que cada render funciona. Los números salen por `--nocapture`.
//!
//! `#[ignore]`d por el mismo motivo que `pdf-render`'s `perf_large_fixture`:
//! genera fixtures pesados y mide wall-clock. Correr explícito:
//!
//! ```sh
//! cargo test --release -p pdf-save --test perf_edit_reopen -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gen_fixtures::large::{generate_large_fixture, LargeFixtureSpec, PERF_LARGE_SPEC};
use pdf_document::{
    Annotation, AnnotationId, AnnotationKind, Color, Command, Document, PageId, Rect,
};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderOptions};
use pdf_save::{save_document, SaveInput, SaveIntent};

/// DPI de una página visible a tamaño de lectura — el mismo que usa el perf
/// harness de `pdf-render` para su render de página 1.
const RENDER_DPI: u32 = 150;

/// Cuántos edits encadenados se miden por documento. Más de uno porque el
/// costo por edit puede crecer con el tamaño del `AnnotationSet` acumulado:
/// un único edit no lo mostraría.
const EDITS: usize = 5;

struct Shape {
    label: &'static str,
    /// Nombre bajo `tests/fixtures/large/`. La forma grande reusa el fixture
    /// del harness de `pdf-render` en vez de duplicar ~50MB en disco.
    file: &'static str,
    spec: LargeFixtureSpec,
}

/// Tres tamaños porque la pregunta no es "cuánto tarda" sino "con qué escala".
/// `save` y `reopen` son sospechosos de ser O(bytes del archivo) y
/// `document_from_lopdf` de ser O(páginas): un solo tamaño no distingue un
/// costo fijo tolerable de uno que explota en documentos reales.
const SHAPES: &[Shape] = &[
    Shape {
        label: "small (10 pg)",
        file: "edit_reopen_10pg.pdf",
        spec: LargeFixtureSpec {
            pages: 10,
            image_side_px: 100,
        },
    },
    Shape {
        label: "medium (50 pg)",
        file: "edit_reopen_50pg.pdf",
        spec: LargeFixtureSpec {
            pages: 50,
            image_side_px: 316,
        },
    },
    Shape {
        label: "large (200 pg)",
        file: "perf_200pg.pdf",
        spec: PERF_LARGE_SPEC,
    },
];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/large")
}

fn ensure_fixture(shape: &Shape) -> PathBuf {
    let path = fixtures_dir().join(shape.file);
    if !path.exists() {
        generate_large_fixture(&path, &shape.spec).expect("generate fixture");
    }
    path
}

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

/// Un highlight chico en la página visible: el edit más barato de la toolbar
/// de T-047. Si el round-trip ya no cierra con esto, no cierra con nada.
fn highlight(id: u64, page: u32) -> Annotation {
    Annotation {
        id: AnnotationId(id),
        page: PageId(page),
        kind: AnnotationKind::Highlight {
            rect: Rect {
                x: 72.0,
                y: 72.0,
                width: 120.0,
                height: 16.0,
            },
            color: Color {
                r: 250,
                g: 220,
                b: 0,
            },
        },
    }
}

/// Las patas de un ciclo edit → visible, cronometradas por separado.
struct EditLegs {
    apply: Duration,
    /// Armar el `SaveInput`. Cuando consumía por valor, esto era un clon
    /// completo de `document` + `base` + `original_bytes` en CADA save; ahora
    /// que sólo presta, la columna queda para que se vea que el costo se fue.
    input: Duration,
    save: Duration,
    reopen: Duration,
    render: Duration,
    close: Duration,
    saved_bytes: usize,
}

impl EditLegs {
    /// Lo que el round-trip AGREGA sobre un repintado normal: todo menos el
    /// `render_page`, que el shell paga igual al hacer scroll.
    fn overhead(&self) -> Duration {
        self.apply + self.input + self.save + self.reopen + self.close
    }
}

fn render_visible_page(renderer: &PdfiumRenderer, handle: DocumentHandle) -> Duration {
    let start = Instant::now();
    let page = renderer
        .render_page(
            handle,
            0,
            RENDER_DPI,
            None,
            RenderOptions::default(),
            Priority::Visible,
        )
        .wait()
        .expect("render visible page");
    let elapsed = start.elapsed();
    assert!(
        page.width().expect("rendered page must expose a width") > 0,
        "a render that returns an empty bitmap would make every timing above meaningless"
    );
    elapsed
}

fn measure(shape: &Shape) {
    let path = ensure_fixture(shape);
    let original_bytes = std::fs::read(&path).expect("read fixture");
    let renderer = PdfiumRenderer::new();

    // Estado que el shell YA tiene hoy: sólo el handle de pdfium.
    let mut handle = renderer
        .open_document_from_bytes(original_bytes.clone(), None)
        .expect("open fixture");

    // Repintado normal, sin ningún edit de por medio: la vara de comparación.
    let baseline = render_visible_page(&renderer, handle);

    // Estado que un shell que EDITA tiene que sostener ADEMÁS del handle: el
    // árbol lopdf (base del writer incremental) y el modelo de documento donde
    // vive el EditLog. Se paga una vez, al abrir.
    let start = Instant::now();
    let (base, security) = pdf_manip::open_document(&path, None).expect("open with lopdf");
    let mut document = pdf_save::document_from_lopdf(&base, security).expect("build edit model");
    let model_load = start.elapsed();

    let page0 = document.pages[0].id;
    let mut legs = Vec::with_capacity(EDITS);

    for edit in 0..EDITS {
        let start = Instant::now();
        apply_command(
            &mut document,
            Command::AddAnnotation(highlight(edit as u64 + 1, page0.0)),
        );
        let apply = start.elapsed();

        // Cada save rebasea TODO el set de edits sobre los bytes originales,
        // así que el modelo y la base tienen que sobrevivirlo. `SaveInput`
        // sólo los presta, así que armarlo es gratis — antes de eso el shell
        // pagaba un clon completo de ambos en CADA save.
        let start = Instant::now();
        let input = SaveInput {
            document: &document,
            base: &base,
            original_bytes: Some(&original_bytes),
            intent: SaveIntent::Default,
        };
        let input_build = start.elapsed();

        let start = Instant::now();
        let saved = save_document(input).expect("save annotated document");
        let save = start.elapsed();
        assert!(
            saved.len() > original_bytes.len(),
            "an incremental save must append a revision, not shrink the file"
        );

        // El largo se captura ANTES de mover los bytes: `open_document_from_bytes`
        // toma ownership y pdfium los retiene mientras viva el documento, que es
        // exactamente lo que hace un shell. Clonarlos acá para poder leer
        // `.len()` después metería una copia del archivo entero dentro de la
        // medición de `reopen`.
        let saved_bytes = saved.len();

        let start = Instant::now();
        let new_handle = renderer
            .open_document_from_bytes(saved, None)
            .expect("reopen saved bytes");
        let reopen = start.elapsed();

        let render = render_visible_page(&renderer, new_handle);

        let start = Instant::now();
        renderer.close_document(handle).expect("close stale handle");
        let close = start.elapsed();

        handle = new_handle;
        legs.push(EditLegs {
            apply,
            input: input_build,
            save,
            reopen,
            render,
            close,
            saved_bytes,
        });
    }

    renderer.close_document(handle).expect("close last handle");

    // Los edits tienen que haber aterrizado de verdad: si el documento
    // guardado no los tiene, los tiempos miden un no-op.
    let final_input = SaveInput {
        document: &document,
        base: &base,
        original_bytes: Some(&original_bytes),
        intent: SaveIntent::Default,
    };
    let final_bytes = save_document(final_input).expect("final save");
    let reloaded = lopdf::Document::load_mem(&final_bytes).expect("reload final save");
    let page1_id = *reloaded.get_pages().get(&1).expect("page 1 must exist");
    let annots = reloaded
        .get_dictionary(page1_id)
        .expect("page 1 dictionary")
        .get(b"Annots")
        .and_then(|o| o.as_array())
        .expect("page 1 must carry /Annots after the edits");
    assert_eq!(
        annots.len(),
        EDITS,
        "every applied annotation must reach the saved file"
    );

    report(shape, &original_bytes, baseline, model_load, &legs);
}

fn report(
    shape: &Shape,
    original_bytes: &[u8],
    baseline: Duration,
    model_load: Duration,
    legs: &[EditLegs],
) {
    let mib = original_bytes.len() as f64 / (1024.0 * 1024.0);
    println!("\n=== {} — {:.1} MiB on disk ===", shape.label, mib);
    println!(
        "baseline repaint (render_page @{RENDER_DPI}dpi, no edit): {:?}",
        baseline
    );
    println!(
        "edit-model load (lopdf open + document_from_lopdf, once per open): {:?}",
        model_load
    );
    println!(
        "{:>4}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}  {:>11}  {:>8}",
        "edit", "apply", "input", "save", "reopen", "close", "render", "overhead", "vs base"
    );
    for (index, leg) in legs.iter().enumerate() {
        let ratio = leg.overhead().as_secs_f64() / baseline.as_secs_f64().max(f64::EPSILON);
        println!(
            "{:>4}  {:>9?}  {:>9?}  {:>9?}  {:>9?}  {:>9?}  {:>9?}  {:>11?}  {:>7.1}x",
            index + 1,
            leg.apply,
            leg.input,
            leg.save,
            leg.reopen,
            leg.close,
            leg.render,
            leg.overhead(),
            ratio
        );
    }

    let total: Duration = legs.iter().map(EditLegs::overhead).sum();
    let mean = total / legs.len() as u32;
    println!(
        "mean round-trip overhead per edit: {:?} ({:.1}x a plain repaint)",
        mean,
        mean.as_secs_f64() / baseline.as_secs_f64().max(f64::EPSILON)
    );
    println!(
        "saved size after {} edits: {:.1} MiB (original {:.1} MiB)",
        legs.len(),
        legs.last().map(|l| l.saved_bytes).unwrap_or(0) as f64 / (1024.0 * 1024.0),
        mib
    );
}

#[test]
#[ignore = "perf harness: generates large fixtures and measures wall-clock; run explicitly, see module docs"]
fn annotation_edit_round_trip_cost_by_document_size() {
    for shape in SHAPES {
        measure(shape);
    }
}
