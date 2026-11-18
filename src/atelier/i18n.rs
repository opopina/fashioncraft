//! Interface language for the menu bar, the view tool rows and their own labels. English is
//! the source text; other languages look it up and fall back to English.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Language {
    #[default]
    English,
    Spanish,
}

impl Language {
    pub(super) const ALL: [Self; 2] = [Self::English, Self::Spanish];
    /// The language's own name.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Spanish => "Español",
        }
    }
}

/// `text` in `language`; English text when there is no translation.
pub(super) fn tr(language: Language, text: &'static str) -> &'static str {
    match language {
        Language::English => text,
        Language::Spanish => spanish(text).unwrap_or(text),
    }
}

/// `text` in `language` for text known only at run time, such as an edit's name; English
/// text when there is no translation.
pub(super) fn tr_any(language: Language, text: &str) -> &str {
    match language {
        Language::English => text,
        Language::Spanish => spanish(text).unwrap_or(text),
    }
}

/// Status messages that carry data, as templates: each `{}` stands for a value, kept as it
/// is in the translation, in the same order.
const STATUS_TEMPLATES: &[(&str, &str)] = &[
    (
        "Avatar made to {} / {} / {} / {} / {} / {} cm · confirm to replace the current one",
        "Avatar hecho a {} / {} / {} / {} / {} / {} cm · confirma para sustituir el actual",
    ),
    (
        "Graphic {} lies across the cut; move it first",
        "El gráfico {} queda sobre el corte; muévelo antes",
    ),
    (
        "Trim {} lies across the cut; move it first",
        "El avío {} queda sobre el corte; muévelo antes",
    ),
    (
        "Tack {} lies across the cut; move it first",
        "El hilván {} queda sobre el corte; muévelo antes",
    ),
    (
        "Graphic {} lies outside the new outline; move it first",
        "El gráfico {} queda fuera del contorno nuevo; muévelo antes",
    ),
    (
        "Trim {} lies outside the new outline; move it first",
        "El avío {} queda fuera del contorno nuevo; muévelo antes",
    ),
    (
        "Tack {} lies outside the new outline; move it first",
        "El hilván {} queda fuera del contorno nuevo; muévelo antes",
    ),
    (
        "Internal line {} lies outside the new outline; delete it first",
        "La línea interna {} queda fuera del contorno nuevo; bórrala antes",
    ),
    (
        "Capturing the take… frame {} of {}",
        "Capturando la toma… fotograma {} de {}",
    ),
    (
        "Copied {} panels and {} internal seams · Ctrl+V to paste",
        "Copiadas {} piezas y {} costuras internas · Ctrl+V para pegar",
    ),
    (
        "Exported {}: {} vertices, {} faces, {} textures",
        "Exportado {}: {} vértices, {} caras, {} texturas",
    ),
    ("Exported {}", "Exportado {}"),
    ("Guide at {}", "Guía en {}"),
    ("Guide moved to {}", "Guía movida a {}"),
    (
        "Internal line → {} · now click inside this panel",
        "Línea interna → {} · ahora haz clic dentro de esta pieza",
    ),
    ("Key set at {} s", "Clave puesta en {} s"),
    (
        "MDD cache saved: {} frames in {}",
        "Caché MDD guardada: {} fotogramas en {}",
    ),
    ("Motion: {}", "Movimiento: {}"),
    (
        "Moving into the pose over {} steps…",
        "Pasando a la pose en {} pasos…",
    ),
    ("Moving into the pose… {} %", "Pasando a la pose… {} %"),
    (
        "Not superimposed: the pieces would start intersecting ({})",
        "No se superpone: las piezas empezarían cruzándose ({})",
    ),
    (
        "OBJ sequence saved: {} frames of the cloth and the avatar in {}",
        "Secuencia OBJ guardada: {} fotogramas de la tela y el avatar en {}",
    ),
    (
        "Open failed; current document retained: {}",
        "No se pudo abrir; se conserva el documento actual: {}",
    ),
    (
        "Point cache saved: {} frames in {}",
        "Caché de puntos guardada: {} fotogramas en {}",
    ),
    (
        "Preferences could not be saved: {}",
        "No se pudieron guardar las preferencias: {}",
    ),
    ("Reaching with the {}", "Alcanzando con {}"),
    (
        "Recovery save failed: {}",
        "Falló la copia de recuperación: {}",
    ),
    ("Report failed: {}", "Falló el informe: {}"),
    (
        "Report saved · path copied · {}",
        "Informe guardado · ruta copiada · {}",
    ),
    ("Save failed: {}", "No se pudo guardar: {}"),
    (
        "Selection inverted: {} selected",
        "Selección invertida: {} seleccionadas",
    ),
    (
        "View “{}” saved with the project",
        "Vista «{}» guardada con el proyecto",
    ),
    ("View “{}” updated", "Vista «{}» actualizada"),
    ("View “{}” deleted", "Vista «{}» borrada"),
    (
        "{} back on the arrangement · the rest keeps its drape",
        "{} de vuelta en la colocación · el resto conserva su drapeado",
    ),
    (
        "{} panels selected · drag to arrange in 2D · Ctrl+C to copy",
        "{} piezas seleccionadas · arrastra para colocarlas en 2D · Ctrl+C para copiar",
    ),
    (
        "{} panels selected · {} · {} · moved pieces keep their drape",
        "{} piezas seleccionadas · {} · {} · las piezas movidas conservan su drapeado",
    ),
    (
        "{}: select a piece, then click the point to place it",
        "{}: selecciona una pieza y haz clic en el punto para colocarla",
    ),
    (
        "Motion imported: {} keys over {} s from {}",
        "Movimiento importado: {} claves en {} s de {}",
    ),
    (
        "{} added · the points stay where they are",
        "{} añadidos · los puntos se quedan donde están",
    ),
];

/// The values `text` puts where `template` has `{}`, when it fits the template.
fn fill_in<'a>(template: &str, text: &'a str) -> Option<Vec<&'a str>> {
    let parts: Vec<&str> = template.split("{}").collect();
    let mut rest = text.strip_prefix(parts[0])?;
    let mut values = Vec::with_capacity(parts.len() - 1);
    for (k, part) in parts.iter().enumerate().skip(1) {
        let at = if k + 1 == parts.len() {
            // The last literal closes the text.
            if !rest.ends_with(part) {
                return None;
            }
            rest.len() - part.len()
        } else {
            rest.find(part)?
        };
        if part.is_empty() && k + 1 < parts.len() {
            return None;
        }
        values.push(&rest[..at]);
        rest = &rest[at + part.len()..];
    }
    (rest.is_empty() && values.iter().all(|v| !v.is_empty())).then_some(values)
}

/// A status message in `language`: its translation when there is one, a translated
/// template filled with the message's own values when it fits one, else as it is.
pub(super) fn status(language: Language, text: &str) -> std::borrow::Cow<'_, str> {
    if language == Language::English {
        return text.into();
    }
    if let Some(t) = spanish(text) {
        return t.into();
    }
    for (template, translation) in STATUS_TEMPLATES {
        if let Some(values) = fill_in(template, text) {
            let mut out = String::new();
            for (k, piece) in translation.split("{}").enumerate() {
                if k > 0 {
                    out.push_str(&counted_nouns(values[k - 1]));
                }
                out.push_str(piece);
            }
            return out.into();
        }
    }
    text.into()
}

/// A counted value such as "3 pieces" or "1 pin" with its noun in Spanish.
fn counted_nouns(value: &str) -> String {
    const NOUNS: [(&str, &str); 8] = [
        ("pieces", "piezas"),
        ("piece", "pieza"),
        ("pins", "alfileres"),
        ("pin", "alfiler"),
        ("panels", "piezas"),
        ("panel", "pieza"),
        ("frames", "fotogramas"),
        ("frame", "fotograma"),
    ];
    match value.split_once(' ') {
        Some((number, noun)) if number.chars().all(|c| c.is_ascii_digit()) => NOUNS
            .iter()
            .find(|(en, _)| *en == noun)
            .map_or_else(|| value.to_string(), |(_, es)| format!("{number} {es}")),
        _ => value.to_string(),
    }
}

fn spanish(text: &str) -> Option<&'static str> {
    Some(match text {
        "File" => "Archivo",
        "Edit" => "Editar",
        "3D Garment" => "Prenda 3D",
        "2D Pattern" => "Patrón 2D",
        "Sewing" => "Costura",
        "Materials" => "Materiales",
        "Avatar" => "Avatar",
        "Animation" => "Animación",
        "Animation editor" => "Editor de animación",
        "Record" => "Grabar",
        "Delete animation" => "Borrar animación",
        "Video capture…" => "Captura de vídeo…",
        "To the start" => "Al inicio",
        "Play" => "Reproducir",
        "Pause" => "Pausa",
        "frame" => "fotograma",
        "frames" => "fotogramas",
        "Playback speed" => "Velocidad de reproducción",
        "Loop" => "Bucle",
        "Set key" => "Poner clave",
        "Key the pose being edited in the Pose window, or the avatar's pose, at this time" => {
            "Pone como clave en este instante la pose que se edita en la ventana Pose o, si no, la del avatar"
        }
        "Delete key" => "Borrar clave",
        "Walk in place" => "Caminar en el sitio",
        "Raise arms" => "Subir los brazos",
        "Wave" => "Saludar",
        "Twist" => "Girar el torso",
        "Use motion" => "Usar movimiento",
        "Replace the keys with this motion, starting from the avatar's pose" => {
            "Sustituye las claves por este movimiento, a partir de la pose del avatar"
        }
        "Add marker" => "Añadir marcador",
        "Frames recorded per second" => "Fotogramas grabados por segundo",
        "Stop" => "Detener",
        "Simulate the cloth while the avatar moves, from the drape as it is now" => {
            "Simula la tela mientras el avatar se mueve, desde el drapeado actual"
        }
        "Take" => "Toma",
        "Clear take" => "Borrar toma",
        "T-shirt from measurements…" => "Camiseta por medidas…",
        "MDD cache…" => "Caché MDD…",
        "MDD cache (Maya/3ds Max)…" => "Caché MDD (Maya/3ds Max)…",
        "Render" => "Render",
        "Display" => "Mostrar",
        "Settings" => "Ajustes",
        "Window" => "Ventana",
        "Help" => "Ayuda",
        "New project" => "Nuevo proyecto",
        "Open project…" => "Abrir proyecto…",
        "Open" => "Abrir",
        "Garment…" => "Prenda…",
        "Trim…" => "Avío…",
        "Avatar…" => "Avatar…",
        "Pose…" => "Pose…",
        "Scene and props…" => "Escena y objetos…",
        "Add" => "Añadir",
        "Project…" => "Proyecto…",
        "Save project" => "Guardar proyecto",
        "Save as" => "Guardar como",
        "Import" => "Importar",
        "SVG patterns…" => "Patrones SVG…",
        "DXF patterns…" => "Patrones DXF…",
        "OBJ…" => "OBJ…",
        "FBX…" => "FBX…",
        "glTF…" => "glTF…",
        "Alembic…" => "Alembic…",
        "USD…" => "USD…",
        "Point cache…" => "Caché de puntos…",
        "Python script…" => "Script de Python…",
        "Export" => "Exportar",
        "Session files (OBJ, patterns, previews)" => {
            "Archivos de la sesión (OBJ, patrones, vistas previas)"
        }
        "OBJ (selected)…" => "OBJ (selección)…",
        "glTF / GLB…" => "glTF / GLB…",
        "OBJ sequence…" => "Secuencia OBJ…",
        "Snapshot" => "Captura",
        "2D patterns at 1:1…" => "Patrones 2D a escala 1:1…",
        "3D view…" => "Vista 3D…",
        "Recover snapshot…" => "Recuperar instantánea…",
        "Garment templates…" => "Plantillas de prendas…",
        "Examples" => "Ejemplos",
        "T-shirt" => "Camiseta",
        "Dress" => "Vestido",
        "Undo" => "Deshacer",
        "Redo" => "Rehacer",
        "Delete" => "Borrar",
        "Copy" => "Copiar",
        "Paste" => "Pegar",
        "Select all" => "Seleccionar todo",
        "Select all seams" => "Seleccionar todas las costuras",
        "Invert selection" => "Invertir selección",
        "Lock" => "Bloquear",
        "Lock selected pieces" => "Bloquear piezas seleccionadas",
        "Lock all pieces" => "Bloquear todas las piezas",
        "Unlock all pieces" => "Desbloquear todas las piezas",
        "Lock internal lines" => "Bloquear líneas internas",
        "Unlock internal lines" => "Desbloquear líneas internas",
        "Simulate" => "Simular",
        "Advance one step" => "Avanzar un paso",
        "High-resolution garment" => "Prenda en alta resolución",
        "Low-resolution garment" => "Prenda en baja resolución",
        "Custom resolution…" => "Resolución personalizada…",
        "Pieces" => "Piezas",
        "Archive" => "Archivar",
        "Deactivate" => "Desactivar",
        "Deactivate with seams" => "Desactivar con sus costuras",
        "Activate" => "Activar",
        "Freeze" => "Congelar",
        "Unfreeze" => "Descongelar",
        "Strengthen" => "Reforzar",
        "Solidify" => "Solidificar",
        "Flip normals" => "Invertir normales",
        "Convert to quads" => "Convertir a cuadriláteros",
        "Convert to triangles" => "Convertir a triángulos",
        "Superimpose over" => "Superponer encima",
        "Superimpose under" => "Superponer debajo",
        "Superimpose side" => "Superponer al lado",
        "Move in 3D" => "Mover en 3D",
        "Rotate in 3D" => "Girar en 3D",
        "Grab cloth" => "Agarrar tela",
        "Select mesh (brush)" => "Seleccionar malla (pincel)",
        "Select mesh (box)" => "Seleccionar malla (caja)",
        "Select mesh (lasso)" => "Seleccionar malla (lazo)",
        "Pins" => "Alfileres",
        "Pin (box)" => "Alfiler (caja)",
        "Pin (lasso)" => "Alfiler (lazo)",
        "Attach all pins to avatar" => "Fijar todos los alfileres al avatar",
        "Detach all pins from avatar" => "Soltar todos los alfileres del avatar",
        "Style lines" => "Líneas de estilo",
        "3D pen" => "Lápiz 3D",
        "Fold arrangement" => "Colocación doblada",
        "Reset 2D arrangement (all)" => "Restablecer colocación 2D (todo)",
        "Reset 3D arrangement (all)" => "Restablecer colocación 3D (todo)",
        "Reset 3D arrangement (selected)" => "Restablecer colocación 3D (selección)",
        "Re-drape 3D arrangement" => "Volver a drapear la colocación 3D",
        "Garment measurements" => "Medidas de la prenda",
        "Linear measure" => "Medida lineal",
        "Circumference measure" => "Medida de contorno",
        "Tear" => "Rasgar",
        "Edit tears" => "Editar rasgados",
        "Tear seam" => "Rasgar costura",
        "Mark simulation issue" => "Marcar problema de simulación",
        "Transform pattern" => "Transformar patrón",
        "Duplicate piece" => "Duplicar pieza",
        "Add point / split line" => "Añadir punto / dividir línea",
        "Smooth curve" => "Suavizar curva",
        "Cut and extend" => "Cortar y extender",
        "Create" => "Crear",
        "Polygon" => "Polígono",
        "Freehand" => "A mano alzada",
        "Rectangle" => "Rectángulo",
        "Ellipse" => "Elipse",
        "Arc sector" => "Sector de arco",
        "Spiral" => "Espiral",
        "Internal polygon or line" => "Polígono o línea interna",
        "Internal rectangle" => "Rectángulo interno",
        "Internal ellipse" => "Elipse interna",
        "Dart" => "Pinza",
        "Trace" => "Calcar",
        "Measure on the board" => "Medir en el tablero",
        "Pleats" => "Pliegues",
        "Single pleat" => "Pliegue simple",
        "Pleat sewing" => "Costura de pliegues",
        "Auto fit" => "Ajuste automático",
        "Segment sewing" => "Costura por segmento",
        "Free sewing" => "Costura libre",
        "M:N segment sewing" => "Costura M:N por segmento",
        "M:N free sewing" => "Costura M:N libre",
        "Auto sewing" => "Costura automática",
        "Check sewing length" => "Comprobar la longitud de costura",
        "Activate all seams" => "Activar todas las costuras",
        "Texture" => "Textura",
        "Edit texture (2D)" => "Editar textura (2D)",
        "Edit texture (3D)" => "Editar textura (3D)",
        "Refresh textures" => "Actualizar texturas",
        "Graphics" => "Gráficos",
        "Transform graphic" => "Transformar gráfico",
        "Graphic on the selected piece…" => "Gráfico en la pieza seleccionada…",
        "Graphic on the 3D garment" => "Gráfico en la prenda 3D",
        "Buttons" => "Botones",
        "Button" => "Botón",
        "Buttonhole" => "Ojal",
        "Fasten buttons" => "Abrochar botones",
        "Zipper" => "Cremallera",
        "Piping" => "Vivo",
        "Tape" => "Cinta",
        "Topstitch" => "Pespunte",
        "Segment topstitch" => "Pespunte por segmento",
        "Free topstitch" => "Pespunte libre",
        "Seamline topstitch" => "Pespunte en la costura",
        "Puckering" => "Fruncido",
        "Steam" => "Vapor",
        "Seam taping" => "Cinta de costura",
        "Delete avatar" => "Borrar avatar",
        "Delete scene and props" => "Borrar escena y objetos",
        "Avatar measurements" => "Medidas del avatar",
        "Tape measure" => "Cinta métrica",
        "Height measure" => "Medida de altura",
        "Play animation" => "Reproducir animación",
        "Avatar editor…" => "Editor de avatar…",
        "High-quality 3D view" => "Vista 3D de alta calidad",
        "Technical view" => "Vista técnica",
        "Viewpoint" => "Punto de vista",
        "Front" => "Frente",
        "Three-quarter, avatar's left" => "Tres cuartos, lado izquierdo del avatar",
        "Back" => "Espalda",
        "Avatar's right side" => "Lado derecho del avatar",
        "Avatar's left side" => "Lado izquierdo del avatar",
        "Top" => "Arriba",
        "Bottom" => "Abajo",
        "Three-quarter, avatar's right" => "Tres cuartos, lado derecho del avatar",
        "Frame selection" => "Encuadrar la selección",
        "3D garment" => "Prenda 3D",
        "Show garment" => "Mostrar prenda",
        "Show pins" => "Mostrar alfileres",
        "Show piece names" => "Mostrar nombres de piezas",
        "Show internal lines" => "Mostrar líneas internas",
        "Show graphics" => "Mostrar gráficos",
        "Show trims" => "Mostrar avíos",
        "Show avatar" => "Mostrar avatar",
        "Show arrangement points" => "Mostrar puntos de colocación",
        "Show joints" => "Mostrar articulaciones",
        "Garment style" => "Estilo de la prenda",
        "Textured with thickness" => "Con textura y grosor",
        "Textured" => "Con textura",
        "Plain colour" => "Color liso",
        "Translucent" => "Translúcida",
        "Random piece colours" => "Colores aleatorios por pieza",
        "Fit maps" => "Mapas de ajuste",
        "Strain map" => "Mapa de deformación",
        "Stress map" => "Mapa de tensión",
        "Pressure map" => "Mapa de presión",
        "Isolate selection" => "Aislar la selección",
        "Gizmo" => "Gizmo",
        "Screen axes" => "Ejes de pantalla",
        "Local axes" => "Ejes locales",
        "World axes" => "Ejes globales",
        "2D snapping" => "Ajuste en 2D",
        "Smart guides" => "Guías inteligentes",
        "Snap to pattern" => "Ajustar al patrón",
        "Snap to grid" => "Ajustar a la rejilla",
        "Grid properties…" => "Propiedades de la rejilla…",
        "Camera properties…" => "Propiedades de la cámara…",
        "Simulation properties…" => "Propiedades de la simulación…",
        "Language" => "Idioma",
        "English" => "English",
        "Español" => "Español",
        "Preferences…" => "Preferencias…",
        "Split view" => "Vista dividida",
        "3D only" => "Solo 3D",
        "2D only" => "Solo 2D",
        "Properties inspector" => "Inspector de propiedades",
        "Library" => "Biblioteca",
        "Object browser" => "Navegador de objetos",
        "History" => "Historial",
        "Reset layout" => "Restablecer la distribución",
        "User manual" => "Manual de usuario",
        "What's new" => "Novedades",
        "Select and move" => "Seleccionar y mover",
        "Grab cloth · pins and tacks" => "Agarrar tela · alfileres e hilvanes",
        "Keyboard map" => "Mapa de teclas",
        "Tool rail" => "Barra lateral de herramientas",
        "Workspace profiles…" => "Perfiles del espacio de trabajo…",
        "Compatible" => "Compatible",
        "ClothCraft classic" => "ClothCraft clásico",
        "Not built yet" => "Aún no está hecho",
        "Cut and sew along line" => "Cortar y coser por la línea",
        "Replace outline with shape" => "Reemplazar contorno por la forma",
        "Slash and spread along line" => "Abrir y separar por la línea",
        "Merge pieces along seam" => "Fusionar las piezas por la costura",
        "Unfold across segment" => "Desdoblar por el segmento",
        "Extend line to outline" => "Prolongar la línea hasta el contorno",
        "Diamond dart" => "Pinza rombo",
        "Rest pose" => "Pose de reposo",
        "Commands…" => "Órdenes…",
        "⏸ Pause" => "⏸ Pausa",
        "▶ Sew and simulate" => "▶ Coser y simular",
        "▶ Simulate" => "▶ Simular",
        "3D GARMENT" => "PRENDA 3D",
        "2D PATTERNS" => "PATRONES 2D",
        "PROPERTIES" => "PROPIEDADES",
        // Inspector, windows and tool rows.
        "180° is flat; less folds toward the pieces' front, more away from it" => {
            "180° es plano; menos dobla hacia la cara delantera de las piezas, más hacia el otro lado"
        }
        "1× is the original point spring strength. Higher values pull harder; collision limits still apply. Changes apply to the next drag." => {
            "1× es la fuerza original del muelle de punto. Valores mayores tiran más; los límites de colisión siguen valiendo. Los cambios valen desde el siguiente arrastre."
        }
        "2 m/s is a light breeze, 8 m/s a strong wind" => {
            "2 m/s es una brisa ligera, 8 m/s un viento fuerte"
        }
        "2D measurements" => "Medidas 2D",
        "2D ruler · two clicks · third click: new measurement · Esc: clear · Ctrl: no snap. Board distance, not a 3D surface tape." => {
            "Regla 2D · dos clics · tercer clic: medida nueva · Esc: borrar · Ctrl: sin imán. Distancia en el tablero, no una cinta sobre la superficie 3D."
        }
        "90° is perpendicular; smaller angles lean towards the end" => {
            "90° es perpendicular; ángulos menores se inclinan hacia el final"
        }
        "A 15 mm button at the middle of the piece" => "Un botón de 15 mm en el centro de la pieza",
        "A locked piece cannot be picked or box-selected on the 2D board; the 3D view still reaches it" => {
            "Una pieza bloqueada no se puede elegir ni seleccionar con caja en el tablero 2D; la vista 3D sí llega a ella"
        }
        "A new piece of the same fabric with this outline, next to this one" => {
            "Una pieza nueva de la misma tela con este contorno, junto a esta"
        }
        "A physical floor the cloth can rest on; starts under the avatar's feet" => {
            "Un suelo físico donde la tela puede apoyarse; empieza bajo los pies del avatar"
        }
        "A ratio > 1 adds ease. Adjust the initial placement, especially along a curve." => {
            "Una proporción > 1 añade holgura. Ajusta la colocación inicial, sobre todo a lo largo de una curva."
        }
        "A row of topstitching 4 mm inside this segment" => {
            "Una fila de pespunte 4 mm hacia dentro de este segmento"
        }
        "A spiral strip, e.g. for a ruffle or a cascade" => {
            "Una tira en espiral, p. ej. para un volante o una cascada"
        }
        "Active panel" => "Pieza activa",
        "Add as internal shape" => "Añadir como forma interna",
        "Add at board center" => "Añadir en el centro del tablero",
        "Add button" => "Añadir botón",
        "Add buttonhole" => "Añadir ojal",
        "Add collinear Bézier handles; the outline is unchanged" => {
            "Añadir tiradores Bézier alineados; el contorno no cambia"
        }
        "Add diamond dart" => "Añadir pinza rombo",
        "Add evenly spaced guides from this one" => "Añadir guías a intervalos iguales desde esta",
        "Add graphic…" => "Añadir gráfico…",
        "Add internal shape" => "Añadir forma interna",
        "Add line" => "Añadir línea",
        "Add object…" => "Añadir objeto…",
        "Add spiral at board center" => "Añadir espiral en el centro del tablero",
        "Air damping: how much the air slows the cloth down (1/s)" => {
            "Amortiguación del aire: cuánto frena el aire a la tela (1/s)"
        }
        "Align" => "Alinear",
        "Align to" => "Alinear a",
        "All pieces" => "Todas las piezas",
        "All seam samples" => "Todas las muestras de costura",
        "Allow different lengths · E" => "Permitir largos distintos · E",
        "Allow easing (different lengths)" => "Permitir embebido (largos distintos)",
        "Also leave their seams out of the simulation" => {
            "Dejar también sus costuras fuera de la simulación"
        }
        "Also points on straight lines, under" => "También puntos en tramos rectos, por debajo de",
        "An 18 mm buttonhole at the middle of the piece" => {
            "Un ojal de 18 mm en el centro de la pieza"
        }
        "An 8 mm zipper 6 mm inside this segment" => {
            "Una cremallera de 8 mm a 6 mm hacia dentro de este segmento"
        }
        "Anchors on joints estimated from the avatar's shape." => {
            "Anclas sobre articulaciones estimadas a partir de la forma del avatar."
        }
        "Angle" => "Ángulo",
        "Apply" => "Aplicar",
        "Apply and restart" => "Aplicar y reiniciar",
        "Apply dimensions" => "Aplicar medidas",
        "Apply fabric" => "Aplicar tela",
        "Apply point" => "Aplicar punto",
        "Apply target" => "Aplicar objetivo",
        "Apply to all panels" => "Aplicar a todas las piezas",
        "Arc sweep" => "Barrido del arco",
        "Arrange on body" => "Colocar sobre el cuerpo",
        "At the feet" => "En los pies",
        "Avatar colour" => "Color del avatar",
        "Avatar editor" => "Editor de avatar",
        "Avatar pose" => "Pose del avatar",
        "Avatar silhouette" => "Silueta del avatar",
        "Avatar · Shift+A" => "Avatar · Shift+A",
        "BACKGROUND IMAGE" => "IMAGEN DE FONDO",
        "Bending stiffness across the seam, times the fabric's" => {
            "Rigidez a la flexión a través de la costura, veces la de la tela"
        }
        "Bending stiffness of the crease, times the fabric's" => {
            "Rigidez a la flexión del pliegue, veces la de la tela"
        }
        "Boolean · A = active panel" => "Booleanas · A = pieza activa",
        "Both faces of the cloth, apart by the fabric's look thickness (1 mm without one), joined by a rim" => {
            "Las dos caras de la tela, separadas por el grosor visual de la tela (1 mm si no tiene), unidas por un canto"
        }
        "Both sides" => "Ambos lados",
        "Bring them back where they were, sewn to the pieces still on the board" => {
            "Devolverlas a donde estaban, cosidas a las piezas que siguen en el tablero"
        }
        "COMBINE TWO SELECTED PIECES" => "COMBINAR DOS PIEZAS SELECCIONADAS",
        "Cancel" => "Cancelar",
        "Cancel draft" => "Cancelar trazo",
        "Cancel · Esc" => "Cancelar · Esc",
        "Centre" => "Centro",
        "Centre of the piece" => "Centro de la pieza",
        "Change length" => "Cambiar largo",
        "Check initialization, then place (one undo step)" => {
            "Comprobar el arranque y colocar (un paso de deshacer)"
        }
        "Choose a boundary in either the 2D or 3D view." => {
            "Elige un borde en la vista 2D o en la 3D."
        }
        "Choose folder…" => "Elegir carpeta…",
        "Choose image…" => "Elegir imagen…",
        "Circle: H = W" => "Círculo: alto = ancho",
        "Clean up this piece" => "Limpiar esta pieza",
        "Clean view · hide all annotations" => "Vista limpia · ocultar todas las anotaciones",
        "Clear" => "Borrar",
        "Clear target" => "Borrar objetivo",
        "Click a point on the 2D board to edit its exact position." => {
            "Haz clic en un punto del tablero 2D para editar su posición exacta."
        }
        "Clicks and boxes on the board reach only the pieces" => {
            "Los clics y las cajas en el tablero solo llegan a las piezas"
        }
        "Clone as piece" => "Clonar como pieza",
        "Make this the outline" => "Usar como contorno",
        "Right" => "Derecha",
        "Both" => "Ambas",
        "Slash the piece along this line and turn the chosen part about where the line leaves the outline past its end, opening a wedge at its start" => {
            "Corta la pieza por esta línea y gira la parte elegida alrededor de donde la línea sale del contorno tras su final, abriendo una cuña en su inicio"
        }
        "Replace the piece's outline with this shape; what lies inside keeps its place" => {
            "Reemplaza el contorno de la pieza por esta forma; lo que queda dentro conserva su sitio"
        }
        "Closed internal shapes following the whole outline" => {
            "Formas internas cerradas que siguen todo el contorno"
        }
        "Cloth touching the avatar, in red" => "Tela que toca al avatar, en rojo",
        "ClothCraft project and template JSON" => "Proyecto ClothCraft y plantilla JSON",
        "Collecting physical-step history…" => "Reuniendo el historial de pasos físicos…",
        "Collision distance" => "Distancia de colisión",
        "Contact points" => "Puntos de contacto",
        "Copies" => "Copias",
        "Copy details" => "Copiar detalles",
        "Copy path" => "Copiar ruta",
        "Copy that piece's outline into this one, centred on Centre" => {
            "Copiar el contorno de esa pieza en esta, centrado en el Centro"
        }
        "Create a mirrored partner. Later shape edits update both pieces; undo restores both." => {
            "Crear una pareja reflejada. Los cambios de forma posteriores actualizan ambas piezas; deshacer restaura ambas."
        }
        "Create an independent mirrored copy" => "Crear una copia reflejada independiente",
        "Create and select an independent mirrored copy beside the source" => {
            "Crear y seleccionar una copia reflejada independiente junto a la original"
        }
        "Create and sew a ruffle" => "Crear y coser un volante",
        "Create folder" => "Crear carpeta",
        "Create panel" => "Crear pieza",
        "Curve points" => "Puntos de curva",
        "Curve the panel around the anchor axis, clearing the body across its width and height" => {
            "Curvar la pieza alrededor del eje del ancla, librando el cuerpo a lo ancho y a lo alto"
        }
        "Cut and sew along this line" => "Cortar y coser por esta línea",
        "Cut out as hole" => "Recortar como agujero",
        "Cut the line in two at this point" => "Partir la línea en dos en este punto",
        "DRAW IN 2D" => "DIBUJAR EN 2D",
        "Del removes the selected seam · Ctrl+Z restores it" => {
            "Supr quita la costura seleccionada · Ctrl+Z la restaura"
        }
        "Delete all" => "Borrar todo",
        "Delete all pins" => "Borrar todos los alfileres",
        "Delete line" => "Borrar línea",
        "Delete panel" => "Borrar pieza",
        "Delete point" => "Borrar punto",
        "Delete the seam" => "Borrar la costura",
        "Delete this seam only" => "Borrar solo esta costura",
        "Delete this view" => "Borrar esta vista",
        "Dimensions (mm)" => "Medidas (mm)",
        "Direction of the copies: 0° up, 90° right" => {
            "Dirección de las copias: 0° arriba, 90° derecha"
        }
        "Discard" => "Descartar",
        "Discard settings" => "Descartar ajustes",
        "Distance between the centres of neighbouring copies" => {
            "Distancia entre los centros de copias vecinas"
        }
        "Distance from the segment's start" => "Distancia desde el inicio del segmento",
        "Distance from the skin, measured along the anchor's outward direction" => {
            "Distancia desde la piel, medida en la dirección hacia fuera del ancla"
        }
        "Distribute" => "Distribuir",
        "Done" => "Hecho",
        "Downward acceleration; 9.81 on Earth, 0 for weightless cloth" => {
            "Aceleración hacia abajo; 9,81 en la Tierra, 0 para tela ingrávida"
        }
        "Drag a line on the board to move it with its group." => {
            "Arrastra una línea en el tablero para moverla con su grupo."
        }
        "Drag a new panel outline, release to preview, then Create panel" => {
            "Arrastra el contorno de una pieza nueva, suelta para previsualizar y luego Crear pieza"
        }
        "Drag an axis / ring · Shift: 15° angles · release: commit · Escape: cancel." => {
            "Arrastra un eje / anillo · Shift: ángulos de 15° · soltar: confirmar · Escape: cancelar."
        }
        "Drag out of a ruler to add one (Ctrl: the other axis); drop it back on a ruler to delete it" => {
            "Arrastra desde una regla para añadir una guía (Ctrl: el otro eje); suéltala en una regla para borrarla"
        }
        "Drag the cloth while it simulates" => "Arrastrar la tela mientras simula",
        "Drag the points or handles on the 2D plane." => {
            "Arrastra los puntos o tiradores en el plano 2D."
        }
        "Draw the first panel of a new garment; replaces the current garment when the contour is closed" => {
            "Dibuja la primera pieza de una prenda nueva; reemplaza la prenda actual al cerrar el contorno"
        }
        "Duplicate" => "Duplicar",
        "Earth" => "Tierra",
        "Ease · E" => "Embebido · E",
        "Edges" => "Bordes",
        "Edit either piece. Shape, curves, internal lines and grain update together on release. Placement and fabric stay independent." => {
            "Edita cualquiera de las dos piezas. Forma, curvas, líneas internas e hilo se actualizan juntos al soltar. Colocación y tela siguen siendo independientes."
        }
        "Edit ranges / chain…" => "Editar tramos / cadena…",
        "Elastic" => "Elástico",
        "Enter commits a pair · Ctrl+Enter commits a chain" => {
            "Intro confirma un par · Ctrl+Intro confirma una cadena"
        }
        "Environment" => "Entorno",
        "Even" => "Uniforme",
        "Exact millimetre size; invalid values leave the document unchanged" => {
            "Medida exacta en milímetros; los valores no válidos dejan el documento igual"
        }
        "Explicitly allow unequal lengths. This changes correspondence, not the fabric's elasticity." => {
            "Permitir explícitamente largos distintos. Cambia la correspondencia, no la elasticidad de la tela."
        }
        "Export folder (files replaced on every export):" => {
            "Carpeta de exportación (los archivos se reemplazan en cada exportación):"
        }
        "Export report" => "Exportar informe",
        "Export…" => "Exportar…",
        "Extend to outline" => "Prolongar hasta el contorno",
        "F / Shift+wheel: direction · E: easing · Q: quick actions" => {
            "F / Shift+rueda: dirección · E: embebido · Q: acciones rápidas"
        }
        "F / Shift+wheel: flip direction · Esc: cancel" => {
            "F / Shift+rueda: invertir dirección · Esc: cancelar"
        }
        "Fabric · weight and stretch" => "Tela · peso y estiramiento",
        "Fasten each button to the nearest buttonhole on another piece" => {
            "Abrochar cada botón al ojal más cercano de otra pieza"
        }
        "Field of view" => "Campo de visión",
        "Fill" => "Rellenar",
        "Fill the hole back in, leaving an internal shape along its rim" => {
            "Volver a rellenar el agujero, dejando una forma interna a lo largo de su borde"
        }
        "Final accepted state" => "Estado final aceptado",
        "Fit tolerance" => "Tolerancia de ajuste",
        "Flat placement: this piece takes the shape of its pattern. The other pieces stay in place." => {
            "Colocación plana: esta pieza toma la forma de su patrón. Las demás piezas se quedan donde están."
        }
        "Flip · F" => "Invertir · F",
        "Floor" => "Suelo",
        "Floor height" => "Altura del suelo",
        "Fold" => "Pliegue",
        "Frame the panel" => "Encuadrar la pieza",
        "FreeSewing · parametric patterns" => "FreeSewing · patrones paramétricos",
        "Friction" => "Fricción",
        "From" => "Desde",
        "From the avatar" => "Desde el avatar",
        "From the file's units to metres" => "De las unidades del archivo a metros",
        "GPU compute" => "Cálculo en GPU",
        "Gap between neighbouring pieces" => "Separación entre piezas vecinas",
        "Garment colour" => "Color de la prenda",
        "Garment · Shift+C" => "Prenda · Shift+C",
        "Gather this line to a share of its cut length, like an elastic band sewn along it" => {
            "Fruncir esta línea a una parte de su largo de corte, como un elástico cosido a lo largo"
        }
        "Girths are taken around the torso: bust at the chest, the narrowest waist, the widest hip." => {
            "Los contornos se toman alrededor del tronco: pecho a la altura del pecho, la cintura más estrecha, la cadera más ancha."
        }
        "Go to this view; Restore comes back" => "Ir a esta vista; Restaurar vuelve",
        "Grab" => "Agarrar",
        "Gravity" => "Gravedad",
        "Grid" => "Cuadrícula",
        "Group with…" => "Agrupar con…",
        "Guides" => "Guías",
        "Height" => "Alto",
        "Hide inspector · restore from Workspace menu" => {
            "Ocultar inspector · se recupera desde el menú Ventana"
        }
        "Hold the selected pieces where they are and let other cloth pass through them; seams stay" => {
            "Mantener las piezas seleccionadas donde están y dejar que otra tela las atraviese; las costuras se quedan"
        }
        "Horizontal scale" => "Escala horizontal",
        "Include the avatar and scene objects" => "Incluir el avatar y los objetos de escena",
        "Indicative presets. Elasticity and bending must be calibrated on your fabric." => {
            "Preajustes orientativos. La elasticidad y la flexión deben calibrarse con tu tela."
        }
        "Industrial DXF, GLB export, animated avatars, USD, Alembic, FBX and PDF are not yet supported." => {
            "DXF industrial, exportar GLB, avatares animados, USD, Alembic, FBX y PDF aún no se admiten."
        }
        "Inspect jitter hotspots" => "Inspeccionar zonas de vibración",
        "Inspect problem faces" => "Inspeccionar caras con problemas",
        "Internal damping settles stretching and bending; the piece as a whole still moves freely." => {
            "La amortiguación interna asienta el estiramiento y la flexión; la pieza entera sigue moviéndose libremente."
        }
        "Internal lines from this segment" => "Líneas internas desde este segmento",
        "Internal shapes & holes" => "Formas internas y agujeros",
        "Interval" => "Intervalo",
        "Isolate selection · Shift+I" => "Aislar selección · Shift+I",
        "Keep B" => "Conservar B",
        "Keep adding edges, or Ctrl+Enter to apply" => {
            "Sigue añadiendo bordes, o Ctrl+Intro para aplicar"
        }
        "Keep the selected pieces in their current 3D shape: they still move and collide as a whole. Click again to release them" => {
            "Mantener las piezas seleccionadas con su forma 3D actual: siguen moviéndose y chocando en bloque. Vuelve a hacer clic para soltarlas"
        }
        "Keep this line from stretching, like a stay tape sewn along it" => {
            "Impedir que esta línea se estire, como una cinta de refuerzo cosida a lo largo"
        }
        "LOCK" => "BLOQUEO",
        "Lay the selected pieces on the inside of the pieces they are sewn to" => {
            "Poner las piezas seleccionadas por dentro de las piezas a las que van cosidas"
        }
        "Lay the selected pieces on the outside of the pieces they are sewn to" => {
            "Poner las piezas seleccionadas por fuera de las piezas a las que van cosidas"
        }
        "Layer" => "Capa",
        "Left" => "Izquierda",
        "Length" => "Largo",
        "Limit tension" => "Límite de tensión",
        "Line at" => "Línea en",
        "Line colour" => "Color de línea",
        "Line dragged points and pieces up with the points around them · Ctrl: off while held" => {
            "Alinear los puntos y piezas arrastrados con los puntos de alrededor · Ctrl: desactivado mientras se mantiene"
        }
        "Line width on screen" => "Grosor de línea en pantalla",
        "Linked copy" => "Copia enlazada",
        "Local capture only. Share the exported folder path to request debugging." => {
            "Captura solo local. Comparte la ruta de la carpeta exportada para pedir depuración."
        }
        "Lock all" => "Bloquear todo",
        "Lock selected" => "Bloquear seleccionadas",
        "Locked in 2D" => "Bloqueada en 2D",
        "MIRROR ACTIVE PIECE" => "REFLEJAR LA PIEZA ACTIVA",
        "Make avatar…" => "Hacer avatar…",
        "Make the library folder so you can drop files in it" => {
            "Crear la carpeta de la biblioteca para poder dejar archivos en ella"
        }
        "Make the two pieces meet at an angle across this seam, like a pressed collar or a turned facing" => {
            "Hacer que las dos piezas se encuentren en ángulo en esta costura, como un cuello planchado o una vista vuelta"
        }
        "Make this corner sharp" => "Hacer esta esquina viva",
        "Match length to the" => "Igualar el largo al",
        "Match up at" => "Casar en",
        "Merge the two pieces here" => "Fusionar aquí las dos piezas",
        "Mesh resolution" => "Resolución de malla",
        "Move" => "Mover",
        "Move (G)" => "Mover (G)",
        "Move 3D (G) / Rotate 3D (R) in the toolbar." => {
            "Mover 3D (G) / Rotar 3D (R) en la barra de herramientas."
        }
        "Move earlier in the chain" => "Mover antes en la cadena",
        "Move into pose" => "Pasar a la pose",
        "Move out" => "Mover hacia fuera",
        "Move this boundary to the target length, as one edit" => {
            "Llevar este borde al largo objetivo, en una sola edición"
        }
        "Move this point only; invalid contours and incompatible seams are rejected" => {
            "Mover solo este punto; los contornos no válidos y las costuras incompatibles se rechazan"
        }
        "Moves" => "Movimientos",
        "Multiple edges per side" => "Varios bordes por lado",
        "Negative values move the segment into the piece" => {
            "Los valores negativos meten el segmento en la pieza"
        }
        "New" => "Nuevo",
        "New sheet" => "Hoja nueva",
        "No objects yet." => "Aún no hay objetos.",
        "No saved views yet. They are stored with the project." => {
            "Aún no hay vistas guardadas. Se guardan con el proyecto."
        }
        "No seams. Sewing links two boundaries." => "No hay costuras. Coser une dos bordes.",
        "Normal" => "Normal",
        "Not grouped" => "Sin agrupar",
        "Now" => "Ahora",
        "Off the board and out of the simulation, kept with the project" => {
            "Fuera del tablero y de la simulación, guardadas con el proyecto"
        }
        "Offset" => "Desplazamiento",
        "Offset outline inwards" => "Desplazar el contorno hacia dentro",
        "One distinct colour per piece instead of the fabric colour" => {
            "Un color distinto por pieza en lugar del color de la tela"
        }
        "One object per piece" => "Un objeto por pieza",
        "Opacity" => "Opacidad",
        "Open material, weight and stretch for this piece" => {
            "Abrir material, peso y estiramiento de esta pieza"
        }
        "Open the website to generate a pattern from your measurements. FreeSewing / SVG import is planned; these links do not add pieces to the scene." => {
            "Abre la web para generar un patrón a partir de tus medidas. La importación de FreeSewing / SVG está prevista; estos enlaces no añaden piezas a la escena."
        }
        "Optimize this piece" => "Optimizar esta pieza",
        "Or click either canvas / press Enter to sew" => {
            "O haz clic en cualquiera de los lienzos / pulsa Intro para coser"
        }
        "Other side" => "Otro lado",
        "Outline of" => "Contorno de",
        "Outline offsets and clean-up" => "Desplazamientos del contorno y limpieza",
        "Over" => "Encima",
        "Overlap the pieces on the 2D board. A supplies fabric and placement." => {
            "Superponer las piezas en el tablero 2D. A aporta la tela y la colocación."
        }
        "Overlapping within" => "Superpuestos a menos de",
        "Overlays" => "Superposiciones",
        "Own collision distance" => "Distancia de colisión propia",
        "PATTERN DISPLAY" => "VISUALIZACIÓN DEL PATRÓN",
        "Pack every piece into the unit square of texture space; fabric textures are left out" => {
            "Empaquetar todas las piezas en el cuadrado unidad del espacio de textura; las texturas de tela se omiten"
        }
        "Paint visible cloth · Shift erases · right drag orbits · simulation paused" => {
            "Pinta la tela visible · Shift borra · arrastrar con el botón derecho orbita · simulación en pausa"
        }
        "Parallel copies of this segment inside the piece" => {
            "Copias paralelas de este segmento dentro de la pieza"
        }
        "Pattern outlines: generic ASCII DXF · physical SVG subsets" => {
            "Contornos de patrón: DXF ASCII genérico · subconjuntos SVG físicos"
        }
        "Pause and mark faces around up to 12 strongest detected vertices. Export report includes motion measurements." => {
            "Pausar y marcar caras alrededor de los 12 vértices detectados más fuertes. El informe exportado incluye medidas de movimiento."
        }
        "Pick a segment on another piece…" => "Elige un segmento de otra pieza…",
        "Pick the other segment…" => "Elige el otro segmento…",
        "Piece names" => "Nombres de las piezas",
        "Pieces stay split along seams." => "Las piezas siguen separadas en las costuras.",
        "Pin" => "Alfiler",
        "Place" => "Colocar",
        "Position" => "Posición",
        "Position in the piece, from its top-left corner" => {
            "Posición en la pieza, desde su esquina superior izquierda"
        }
        "Preset" => "Preajuste",
        "Preview" => "Vista previa",
        "Print a PNG image on this piece, 12 cm wide at its middle" => {
            "Estampar una imagen PNG en esta pieza, de 12 cm de ancho en su centro"
        }
        "Print on the back face of the fabric instead" => "Estampar en la cara trasera de la tela",
        "Project and template JSON · garment OBJ · SVG patterns · fiber guides" => {
            "Proyecto y plantilla JSON · OBJ de la prenda · patrones SVG · guías del hilo"
        }
        "Put the first shape in the middle of the piece" => {
            "Poner la primera forma en el centro de la pieza"
        }
        "Quality" => "Calidad",
        "Quick actions · Q" => "Acciones rápidas · Q",
        "Quick · Q" => "Rápido · Q",
        "RULERS & GUIDES" => "REGLAS Y GUÍAS",
        "Ratio" => "Proporción",
        "Real width the image spans; set it from a known measurement to trace at scale" => {
            "Ancho real que abarca la imagen; ajústalo con una medida conocida para calcar a escala"
        }
        "Refresh" => "Actualizar",
        "Remesh" => "Rehacer malla",
        "Remove" => "Quitar",
        "Remove seams, pins, internal lines and target lengths first." => {
            "Quita antes costuras, alfileres, líneas internas y largos objetivo."
        }
        "Remove tacks" => "Quitar hilvanes",
        "Remove the cloth inside this closed shape" => {
            "Quitar la tela dentro de esta forma cerrada"
        }
        "Remove this point and rejoin its two boundaries (Del)" => {
            "Quitar este punto y unir sus dos bordes (Supr)"
        }
        "Remove this segment from the draft" => "Quitar este segmento del trazo",
        "Repeated reversals, not rendering FPS. Real fabric vibration can also trigger this indicator." => {
            "Inversiones repetidas, no FPS de dibujo. La vibración real de la tela también puede activar este indicador."
        }
        "Replace both incident curves by straight boundaries" => {
            "Reemplazar las dos curvas que llegan por bordes rectos"
        }
        "Replace the garment with this T-shirt" => "Reemplazar la prenda por esta camiseta",
        "Replace…" => "Reemplazar…",
        "Replicate" => "Repetir",
        "Reset" => "Restablecer",
        "Reset 3D arrangement" => "Restablecer la colocación 3D",
        "Reset all layers" => "Restablecer todas las capas",
        "Reshape the avatar to these measurements, then confirm it like an imported avatar" => {
            "Remodelar el avatar a estas medidas y confirmarlo como un avatar importado"
        }
        "Rest" => "Reposo",
        "Rest length of every outline segment, in millimetres" => {
            "Largo en reposo de cada segmento del contorno, en milímetros"
        }
        "Restore" => "Restaurar",
        "Reverse" => "Invertir",
        "Reverse side B · F or Shift + wheel over either canvas" => {
            "Invertir el lado B · F o Shift + rueda sobre cualquier lienzo"
        }
        "Reverse the entire B side" => "Invertir todo el lado B",
        "Right-click either canvas to choose a tool" => {
            "Clic derecho en cualquier lienzo para elegir herramienta"
        }
        "Rotate" => "Rotar",
        "Rotate (R)" => "Rotar (R)",
        "Rotate and scale the selected points about their centre" => {
            "Rotar y escalar los puntos seleccionados alrededor de su centro"
        }
        "Rotation" => "Rotación",
        "Round this corner" => "Redondear esta esquina",
        "Ruffle width" => "Ancho del volante",
        "Rulers" => "Reglas",
        "Run each copy up to the outline instead of stopping at the segment's ends" => {
            "Llevar cada copia hasta el contorno en vez de pararla en los extremos del segmento"
        }
        "Run simulation to measure motion." => "Simula para medir el movimiento.",
        "SELECTED SEAM" => "COSTURA SELECCIONADA",
        "Same scale both ways" => "Misma escala en ambos sentidos",
        "Save board as PNG…" => "Guardar el tablero como PNG…",
        "Save current view" => "Guardar la vista actual",
        "Save the 2D board exactly as shown, at screen resolution" => {
            "Guardar el tablero 2D tal como se ve, a la resolución de pantalla"
        }
        "Saved views" => "Vistas guardadas",
        "Scale" => "Escala",
        "Scale piece" => "Escalar pieza",
        "Scale the piece, its holes and internal lines about its centre" => {
            "Escalar la pieza, sus agujeros y sus líneas internas alrededor de su centro"
        }
        "Scene and props" => "Escena y objetos",
        "Seam links" => "Enlaces de costura",
        "Segment" => "Segmento",
        "Segment lengths" => "Largos de los segmentos",
        "Select A then B in either view. Ranges use rest length, in mm." => {
            "Elige A y luego B en cualquier vista. Los tramos usan el largo en reposo, en mm."
        }
        "Select a piece in the list or in a view. Pen / Rectangle to draw; Sew to assemble." => {
            "Elige una pieza en la lista o en una vista. Pluma / Rectángulo para dibujar; Coser para montar."
        }
        "Select a piece to edit its fabric." => "Elige una pieza para editar su tela.",
        "Set exact length" => "Fijar largo exacto",
        "Sewing at cursor" => "Costura bajo el cursor",
        "Sewing audit" => "Revisión de costuras",
        "Sewn boundaries and those with targets, allowances or notches keep their points." => {
            "Los bordes cosidos y los que tienen objetivos, márgenes o piquetes conservan sus puntos."
        }
        "Shapes created in one go, the first one included" => {
            "Formas creadas de una vez, la primera incluida"
        }
        "Shapes, mirrors and booleans near the cursor" => {
            "Formas, reflejos y booleanas cerca del cursor"
        }
        "Shift-click two pieces. The active piece is A." => {
            "Shift+clic en dos piezas. La pieza activa es A."
        }
        "Show only the pieces selected now; press again to show every piece" => {
            "Mostrar solo las piezas seleccionadas ahora; vuelve a pulsar para mostrar todas"
        }
        "Show only the shaded garment and avatar. Keeps your overlay settings for later." => {
            "Mostrar solo la prenda y el avatar sombreados. Guarda tus superposiciones para luego."
        }
        "Show skeleton (X-ray)" => "Mostrar esqueleto (rayos X)",
        "Show the panel on the anchor without changing the document" => {
            "Mostrar la pieza en el ancla sin cambiar el documento"
        }
        "Signed millimetres; typing does not change the pattern" => {
            "Milímetros con signo; escribir no cambia el patrón"
        }
        "Silhouette" => "Silueta",
        "Simulation state" => "Estado de la simulación",
        "Size of the drawn outline" => "Tamaño del contorno dibujado",
        "Size of the supporting ellipse. The sector is closed by two radii." => {
            "Tamaño de la elipse de apoyo. El sector se cierra con dos radios."
        }
        "Size on screen. Zero uses the original point grab. The brush follows the picked piece's surface, avoiding nearby disconnected layers." => {
            "Tamaño en pantalla. Cero usa el agarre de punto original. El pincel sigue la superficie de la pieza elegida y evita capas cercanas que no están unidas."
        }
        "Smooth tangents" => "Tangentes suaves",
        "Solver settings" => "Ajustes del solver",
        "Split" => "Dividir",
        "Split at point" => "Dividir en un punto",
        "Static avatars: OBJ · glTF · GLB · STL · PLY · COLLADA" => {
            "Avatares estáticos: OBJ · glTF · GLB · STL · PLY · COLLADA"
        }
        "Store this required length and set the boundary to it" => {
            "Guardar este largo exigido y llevar el borde a él"
        }
        "Strain map · Alt+7" => "Mapa de deformación · Alt+7",
        "Strain that reaches the last of the eight colours" => {
            "Deformación que alcanza el último de los ocho colores"
        }
        "Strength" => "Fuerza",
        "Stretch under 5 N on a 5 cm strip" => "Estiramiento bajo 5 N en una tira de 5 cm",
        "Superimpose" => "Superponer",
        "Swap A / B" => "Intercambiar A / B",
        "Swap seam ends · F" => "Intercambiar extremos de costura · F",
        "T-shirt from measurements" => "Camiseta por medidas",
        "Tack" => "Hilván",
        "Target" => "Objetivo",
        "Technical view · Alt+0" => "Vista técnica · Alt+0",
        "The highlighted edge is side A. The next edge becomes side B." => {
            "El borde resaltado es el lado A. El siguiente borde será el lado B."
        }
        "The larger the %, the more the fabric stretches. This is not the strain at failure." => {
            "Cuanto mayor el %, más se estira la tela. No es la deformación de rotura."
        }
        "Thick" => "Gruesa",
        "This avatar ships no arrangement anchors (arrangement.json next to its OBJ), and its shape does not allow estimating them: it should stand Y up, facing +Z, arms away from the body." => {
            "Este avatar no trae anclas de colocación (arrangement.json junto a su OBJ) y su forma no permite estimarlas: debe estar de pie con Y arriba, mirando a +Z y con los brazos separados del cuerpo."
        }
        "To" => "Hasta",
        "Topstitching and zippers go along a selected segment." => {
            "El pespunte y las cremalleras van a lo largo de un segmento seleccionado."
        }
        "Trace over a scanned or photographed pattern (PNG or TIFF)" => {
            "Calcar sobre un patrón escaneado o fotografiado (PNG o TIFF)"
        }
        "Transform" => "Transformar",
        "Turn" => "Giro",
        "Turn the image around its centre to square a skewed scan" => {
            "Girar la imagen alrededor de su centro para enderezar un escaneo torcido"
        }
        "Uncommitted drawing and sewing previews are not included in the report." => {
            "Los trazos y las costuras sin confirmar no se incluyen en el informe."
        }
        "Under" => "Debajo",
        "Unfasten" => "Desabrochar",
        "Unfold across this segment" => "Desplegar a través de este segmento",
        "Ungroup" => "Desagrupar",
        "Unified UV" => "UV unificadas",
        "Uniform" => "Uniforme",
        "Units" => "Unidades",
        "Unlink mirror" => "Desenlazar reflejo",
        "Unlock all" => "Desbloquear todo",
        "Up" => "Arriba",
        "Up to outline" => "Hasta el contorno",
        "Use the current average gap, left to right: equal gaps within the same overall width" => {
            "Usar la separación media actual, de izquierda a derecha: separaciones iguales dentro del mismo ancho total"
        }
        "Vertical field of view; also sets the orthographic view's size" => {
            "Campo de visión vertical; también fija el tamaño de la vista ortográfica"
        }
        "Vertical scale" => "Escala vertical",
        "Where the wind comes from: 0° is +Z, 90° is +X, seen from above" => {
            "De dónde viene el viento: 0° es +Z, 90° es +X, visto desde arriba"
        }
        "Width" => "Ancho",
        "Wind" => "Viento",
        "Wind speed" => "Velocidad del viento",
        "Wireframe" => "Malla de alambre",
        "Without its own collision distance a fabric uses the scene's, in Simulation." => {
            "Sin distancia de colisión propia, una tela usa la de la escena, en Simulación."
        }
        "Wrap around body" => "Envolver alrededor del cuerpo",
        "X: left / right · Y: up / down in local pattern coordinates." => {
            "X: izquierda / derecha · Y: arriba / abajo en coordenadas locales del patrón."
        }
        "Z up" => "Z arriba",
        "angle" => "ángulo",
        "every" => "cada",
        "from skin" => "desde la piel",
        "|stretch − 1| against the same range for every fabric" => {
            "|estiramiento − 1| con el mismo rango para todas las telas"
        }
        // Status messages, hints and tabs.
        "180° is flat; less folds toward the piece's front, more away from it" => {
            "180° es plano; menos dobla hacia el derecho de la pieza, más la aleja de él"
        }
        "A diamond hole of this width and height at Centre, its upper and lower legs sewn together: it closes into a dart along the height" => {
            "Un hueco en rombo de este ancho y alto en el centro, con sus lados superior e inferior cosidos: se cierra en una pinza a lo alto"
        }
        "A flat sketch: plain garment colour with outline and silhouette lines" => {
            "Un boceto plano: color liso de la prenda con contorno y líneas de silueta"
        }
        "A steady breeze. Cloth feels pressure drag where it faces the flow, and the same drag slows it as it falls" => {
            "Una brisa constante. La tela nota el arrastre donde da al flujo, y ese mismo arrastre la frena al caer"
        }
        "Add Bézier handles" => "Añadir tiradores Bézier",
        "Anchor" => "Anclaje",
        "Animation deleted" => "Animación borrada",
        "Anisotropy" => "Anisotropía",
        "Appearance fixture has not been saved" => "La muestra de apariencia no se ha guardado",
        "Apply seam" => "Aplicar costura",
        "Archived" => "Archivado",
        "Board capture failed: the board is not on screen" => {
            "No se pudo capturar el tablero: no está en pantalla"
        }
        "Bone" => "Hueso",
        "Both ends move by the same amount" => "Ambos extremos se mueven lo mismo",
        "Brush px" => "Pincel px",
        "Bust, hip, shoulders, length and placement from the current avatar" => {
            "Busto, cadera, hombros, largo y colocación del avatar actual"
        }
        "Capturing the 3D view…" => "Capturando la vista 3D…",
        "Carry this end on straight up to the outline or the nearest line" => {
            "Prolonga este extremo en línea recta hasta el contorno o la línea más cercana"
        }
        "Choose boundaries in either view · F: flip · E: ease" => {
            "Elige bordes en cualquier vista · F: invertir · E: embeber"
        }
        "Choose two boundaries in 2D or 3D, then create the seam in Properties" => {
            "Elige dos bordes en 2D o 3D y crea la costura en Propiedades"
        }
        "Choose two different positions on the boundary" => {
            "Elige dos posiciones distintas del borde"
        }
        "Choose, then drag on the 2D board. Shift makes a circle or square." => {
            "Elige y arrastra en el tablero 2D. Mayús hace un círculo o un cuadrado."
        }
        "Click a boundary to choose its panel, then draw inside · Enter: finish" => {
            "Haz clic en un borde para elegir su pieza y dibuja dentro · Intro: terminar"
        }
        "Click a second segment: the one that differs takes the other's length, moving the end chosen above" => {
            "Haz clic en un segundo segmento: el que difiere toma la longitud del otro, moviendo el extremo elegido arriba"
        }
        "Click another internal line of this piece; grouped lines move together" => {
            "Haz clic en otra línea interna de esta pieza; las líneas agrupadas se mueven juntas"
        }
        "Click on the cloth to pin it" => "Haz clic en la tela para fijarla con un alfiler",
        "Click on the cloth to tack it" => "Haz clic en la tela para hilvanarla",
        "Click the cloth to pin a point where it is · drag a box to pin an area · click a pin to remove it · Cmd+drag removes the pins in a box" => {
            "Haz clic en la tela para fijar un punto donde está · arrastra un recuadro para fijar una zona · haz clic en un alfiler para quitarlo · Cmd+arrastrar quita los alfileres de un recuadro"
        }
        "Click two points of the cloth to hold them together, like a basting stitch" => {
            "Haz clic en dos puntos de la tela para unirlos, como un hilván"
        }
        "Close contour" => "Cerrar contorno",
        "Commit or cancel the multi-edge seam before simulating" => {
            "Confirma o cancela la costura de varios bordes antes de simular"
        }
        "Commit the seam" => "Confirmar la costura",
        "Convert to straight segment" => "Convertir en segmento recto",
        "Coulomb friction coefficient of every contact: cloth on the avatar, on itself and on the floor. 0 lets cloth slide freely" => {
            "Coeficiente de rozamiento de Coulomb de todo contacto: tela sobre el avatar, sobre sí misma y sobre el suelo. 0 deja que la tela resbale libremente"
        }
        "Crease the piece along this line at a set angle, like a pressed pleat" => {
            "Marca un pliegue en la pieza a lo largo de esta línea con un ángulo dado, como una tabla planchada"
        }
        "Create or cancel the freehand panel before simulating" => {
            "Crea o cancela la pieza a mano alzada antes de simular"
        }
        "Cut in two and sewn along the cut" => "Cortada en dos y cosida por el corte",
        "A hole lies across the cut; move the cut line" => {
            "Un agujero queda sobre el corte; mueve la línea de corte"
        }
        "Delete the seams on the outline and on the shape first" => {
            "Borra antes las costuras del contorno y de la forma"
        }
        "Clear the elastic, tape or folds on the outline and on the shape first" => {
            "Quita antes el elástico, la cinta o los pliegues del contorno y de la forma"
        }
        "Only a closed internal shape can become the outline" => {
            "Solo una forma interna cerrada puede ser el contorno"
        }
        "A hole lies outside the new outline; delete it first" => {
            "Un agujero queda fuera del contorno nuevo; bórralo antes"
        }
        "Remove the pins that are not on the new outline's corners first" => {
            "Quita antes los alfileres que no están en las esquinas del contorno nuevo"
        }
        "Release pins, unfreeze and activate the piece before replacing its outline" => {
            "Suelta los alfileres, descongela y activa la pieza antes de reemplazar su contorno"
        }
        "Unlink the mirrored pair before replacing the outline" => {
            "Separa la pareja enlazada antes de reemplazar el contorno"
        }
        "Replacing the outline needs an editable contour" => {
            "Reemplazar el contorno requiere un contorno editable"
        }
        "Outline replaced by the shape" => "Contorno reemplazado por la forma",
        "Wedge opened along the line" => "Cuña abierta por la línea",
        "Open the wedge by 0.1° to 90°" => "Abre la cuña entre 0,1° y 90°",
        "DXF panels added · move and sew them in the workspace · Undo is available" => {
            "Piezas DXF añadidas · muévelas y cóselas en el espacio de trabajo · se puede deshacer"
        }
        "Damping" => "Amortiguación",
        "Diamond dart added and sewn" => "Pinza en rombo añadida y cosida",
        "Distances to the outline from the cursor while drawing an internal line, and from the selected internal line" => {
            "Distancias al contorno desde el cursor al dibujar una línea interna, y desde la línea interna seleccionada"
        }
        "Drag a graphic to move it · its corners scale it · its top handle turns it (Shift: 15°) · Delete removes it" => {
            "Arrastra un gráfico para moverlo · sus esquinas lo escalan · su tirador superior lo gira (Mayús: 15°) · Supr lo quita"
        }
        "Drag points to edit · Shift+click to add · drag empty space to box select · Ctrl+C / Ctrl+V" => {
            "Arrastra puntos para editar · Mayús+clic para añadir · arrastra en vacío para seleccionar con recuadro · Ctrl+C / Ctrl+V"
        }
        "Estimated tension against each fabric's limit: stiff fabrics reach it sooner" => {
            "Tensión estimada frente al límite de cada tejido: los tejidos rígidos lo alcanzan antes"
        }
        "Existing seams" => "Costuras existentes",
        "Export OBJ" => "Exportar OBJ",
        "Export OBJ (selected)" => "Exportar OBJ (selección)",
        "Eyedropper cancelled" => "Cuentagotas cancelado",
        "Eyedropper: no cloth there" => "Cuentagotas: ahí no hay tela",
        "Fabric appearance is unchanged" => "La apariencia del tejido no cambia",
        "Finish line" => "Terminar línea",
        "Finish or cancel the current drawing, placement or sewing before importing patterns" => {
            "Termina o cancela el dibujo, la colocación o la costura en curso antes de importar patrones"
        }
        "First point chosen · click the point to tack it to" => {
            "Primer punto elegido · haz clic en el punto al que hilvanarlo"
        }
        "Flip" => "Invertir",
        "Flipped" => "Invertida",
        "Fold angle" => "Ángulo de pliegue",
        "Free both boundaries without deleting the fabric. Shortcut: Del. Undo: Ctrl+Z." => {
            "Libera ambos bordes sin borrar el tejido. Atajo: Supr. Deshacer: Ctrl+Z."
        }
        "Front view of the avatar behind the pieces, lined up with the first piece that faces the front" => {
            "Vista frontal del avatar detrás de las piezas, alineada con la primera pieza que mira al frente"
        }
        "Gap the simulator keeps between this fabric and the avatar, the floor or other cloth; two fabrics keep the mean of theirs. Off: the scene's Collision distance in Simulation" => {
            "Separación que el simulador mantiene entre este tejido y el avatar, el suelo u otra tela; dos tejidos mantienen la media de las suyas. Desactivada: la distancia de colisión de la escena en Simulación"
        }
        "Gap the solver keeps between cloth and the avatar, the floor and other cloth. Larger is more forgiving with thick layers; smaller fits closer. A fabric can set its own in the Fabric tab; two sides keep the mean" => {
            "Separación que el simulador mantiene entre la tela y el avatar, el suelo y otra tela. Más grande tolera mejor las capas gruesas; más pequeña ajusta más. Un tejido puede fijar la suya en la pestaña Tejido; dos caras mantienen la media"
        }
        "Guide deleted" => "Guía borrada",
        "Guide limit reached; delete some guides first" => {
            "Límite de guías alcanzado; borra algunas antes"
        }
        "Hole cut · fill it back in from the piece inspector" => {
            "Hueco recortado · vuelve a rellenarlo desde el inspector de la pieza"
        }
        "Hole filled · an internal shape keeps its outline" => {
            "Hueco rellenado · una forma interna conserva su contorno"
        }
        "How solid the print looks; the image's own transparency adds to it" => {
            "Cuán opaco se ve el estampado; la transparencia propia de la imagen se suma"
        }
        "In the new pose · the drape followed the body" => {
            "En la nueva pose · el drapeado siguió al cuerpo"
        }
        "Internal shape added · select it to cut it out as a hole" => {
            "Forma interna añadida · selecciónala para recortarla como hueco"
        }
        "Isolation ended: every piece is shown again" => {
            "Aislamiento terminado: se vuelven a mostrar todas las piezas"
        }
        "Iterations" => "Iteraciones",
        "Join the pieces this seam sews edge to edge into one piece; the seam goes away" => {
            "Une en una sola pieza las piezas que esta costura cose borde con borde; la costura desaparece"
        }
        "Lay the selected pieces flat on their arrangement again; the rest keeps its drape" => {
            "Vuelve a extender las piezas seleccionadas en su colocación; el resto conserva su drapeado"
        }
        "Length ×" => "Longitud ×",
        "Limb placed · Move into pose to bring the avatar there" => {
            "Miembro colocado · Mover a la pose para llevar allí el avatar"
        }
        "Mark issue · D" => "Marcar problema · D",
        "Material coordinates in millimetres; the rest origin is re-based without moving the other points" => {
            "Coordenadas del material en milímetros; el origen de reposo se recoloca sin mover los demás puntos"
        }
        "Mesh axis" => "Eje de la malla",
        "Mirror the piece across this straight segment into the whole piece; the segment ends up inside it" => {
            "Refleja la pieza sobre este segmento recto para obtener la pieza entera; el segmento queda dentro"
        }
        "Move the avatar into this pose while simulating: the drape follows the body" => {
            "Lleva el avatar a esta pose mientras simula: el drapeado sigue al cuerpo"
        }
        "New project · one unsewn rectangle" => "Proyecto nuevo · un rectángulo sin coser",
        "No visible cloth inside the box" => "No hay tela visible dentro del recuadro",
        "Number keys: 2 front · 8 back · 4/6 avatar's right/left side · 5 top · 0 bottom · 1/3 three-quarter right/left · F frame" => {
            "Teclas numéricas: 2 frente · 8 espalda · 4/6 lado derecho/izquierdo del avatar · 5 arriba · 0 abajo · 1/3 tres cuartos derecha/izquierda · F encuadrar"
        }
        "Only lines of the same piece can be grouped" => {
            "Solo se pueden agrupar líneas de la misma pieza"
        }
        "Outline copied as an internal shape" => "Contorno copiado como forma interna",
        "Panel holds already match the request" => {
            "Las sujeciones de las piezas ya coinciden con lo pedido"
        }
        "Panel holds updated" => "Sujeciones de las piezas actualizadas",
        "Partial sewing · click the other endpoint on this boundary" => {
            "Costura parcial · haz clic en el otro extremo de este borde"
        }
        "Pause and paint problem faces in 3D" => "Pausa y pinta las caras con problemas en 3D",
        "Pen: click = corner · drag = curve · click the first corner to close" => {
            "Pluma: clic = esquina · arrastrar = curva · clic en la primera esquina para cerrar"
        }
        "Pick partial range (two clicks)" => "Elegir tramo parcial (dos clics)",
        "Pick two different points" => "Elige dos puntos distintos",
        "Pieces arranged · Ctrl+Z undoes it" => "Piezas colocadas · Ctrl+Z lo deshace",
        "Pieces merged along the seam" => "Piezas unidas por la costura",
        "Points arranged · Ctrl+Z undoes it" => "Puntos colocados · Ctrl+Z lo deshace",
        "Pose applied · the drape starts again from the arrangement" => {
            "Pose aplicada · el drapeado vuelve a empezar desde la colocación"
        }
        "Pressure" => "Presión",
        "Pushes the piece along its normal, like air inside a closed garment or a cushion; negative values pull. A few pascals already outweigh the fabric" => {
            "Empuja la pieza según su normal, como el aire dentro de una prenda cerrada o un cojín; los valores negativos tiran. Unos pocos pascales ya pesan más que el tejido"
        }
        "Radius (px)" => "Radio (px)",
        "Rebuild the avatar in this pose at once; the drape starts again" => {
            "Reconstruye el avatar en esta pose de inmediato; el drapeado vuelve a empezar"
        }
        "Rebuild the obstacle with these objects; the drape starts again" => {
            "Reconstruye el obstáculo con estos objetos; el drapeado vuelve a empezar"
        }
        "Recording…" => "Grabando…",
        "Rectangle: drag between two opposite corners" => {
            "Rectángulo: arrastra entre dos esquinas opuestas"
        }
        "Redo · Ctrl+Shift+Z" => "Rehacer · Ctrl+Mayús+Z",
        "Reflect in local pattern coordinates; one undoable edit" => {
            "Refleja en coordenadas locales del patrón; una sola edición que se puede deshacer"
        }
        "Release the pins of that piece before superimposing it" => {
            "Quita los alfileres de esa pieza antes de superponerla"
        }
        "Remove extra segments before switching to single-edge sewing" => {
            "Quita los segmentos de más antes de pasar a costura de un solo borde"
        }
        "Right-click for tools · middle-drag to pan · wheel to zoom" => {
            "Clic derecho para herramientas · arrastre central para desplazar · rueda para ampliar"
        }
        "Ruler origin moved · double-click the corner to reset it" => {
            "Origen de la regla movido · doble clic en la esquina para restablecerlo"
        }
        "Ruler origin reset" => "Origen de la regla restablecido",
        "SVG panels added · move and sew them in the workspace · Undo is available" => {
            "Piezas SVG añadidas · muévelas y cóselas en el espacio de trabajo · se puede deshacer"
        }
        "Scene updated · the drape starts again from the arrangement" => {
            "Escena actualizada · el drapeado vuelve a empezar desde la colocación"
        }
        "Seam created · pick two more edges, or press Space to simulate" => {
            "Costura creada · elige dos bordes más o pulsa Espacio para simular"
        }
        "Seam removed · drape preserved · Ctrl+Z to undo" => {
            "Costura quitada · drapeado conservado · Ctrl+Z para deshacer"
        }
        "Select a piece to put the image on" => "Selecciona una pieza donde poner la imagen",
        "Select a straight contour segment first" => {
            "Selecciona antes un segmento recto del contorno"
        }
        "Select a straight internal line first" => "Selecciona antes una línea interna recta",
        "Select a closed internal shape first" => "Selecciona antes una forma interna cerrada",
        "Select the pieces first" => "Selecciona antes las piezas",
        "Select the pieces to reset" => "Selecciona las piezas que restablecer",
        "Selected pieces can no longer be picked or box-selected on the board" => {
            "Las piezas seleccionadas ya no se pueden elegir ni seleccionar con recuadro en el tablero"
        }
        "Sew" => "Coser",
        "Shape cloned as a piece of the same fabric" => "Forma clonada como pieza del mismo tejido",
        "Share of the curve points kept; the start value keeps this piece within 0.5 mm" => {
            "Proporción de puntos de curva que se conservan; el valor inicial mantiene esta pieza dentro de 0,5 mm"
        }
        "Simulation layer. Where pieces of different layers touch, only the higher (outer) one gives way, so it settles over the lower one. 0: no layer. Reset to 0 once the garment has settled" => {
            "Capa de simulación. Donde se tocan piezas de capas distintas, solo cede la más alta (exterior), que se asienta sobre la más baja. 0: sin capa. Vuelve a 0 cuando la prenda se haya asentado"
        }
        "Size the piece draws in to along this grain direction, in percent of its cut size, as a fabric that shrinks. 100 % is none" => {
            "Tamaño al que encoge la pieza en esta dirección del hilo, en porcentaje de su tamaño de corte, como un tejido que encoge. 100 % es sin encoger"
        }
        "Space the pieces left to right" => "Espaciar las piezas de izquierda a derecha",
        "Space the pieces top to bottom" => "Espaciar las piezas de arriba abajo",
        "Split the piece in two along this line, carried on to the outline, and sew the halves together; seams crossing the cut are split, not lost" => {
            "Divide la pieza en dos por esta línea, prolongada hasta el contorno, y cose las mitades; las costuras que cruzan el corte se dividen, no se pierden"
        }
        "Start moved to this boundary · click its end" => {
            "Inicio movido a este borde · haz clic en su final"
        }
        "Superimpose works on pieces sewn to another piece" => {
            "Superponer funciona con piezas cosidas a otra pieza"
        }
        "Swap the piece's front and back faces: the fabric's back colour shows on the side that showed its front. Only the look changes" => {
            "Intercambia el derecho y el revés de la pieza: el color del revés del tejido se ve en el lado que mostraba el derecho. Solo cambia el aspecto"
        }
        "T-shirt drafted from the measurements" => "Camiseta trazada a partir de las medidas",
        "Tacked · the two points are drawn together" => "Hilvanado · los dos puntos se juntan",
        "Tacks removed" => "Hilvanes quitados",
        "Target length must be a positive finite value" => {
            "La longitud objetivo debe ser un valor finito positivo"
        }
        "Templates · browse original examples or import a native template" => {
            "Plantillas · explora ejemplos originales o importa una plantilla nativa"
        }
        "Tension at which a fabric counts as stretched to its wearable limit (100 %)" => {
            "Tensión a la que un tejido se considera estirado hasta su límite de uso (100 %)"
        }
        "Texture staged · Apply appearance to show it" => {
            "Textura preparada · Aplicar apariencia para mostrarla"
        }
        "That piece turns and moves so its segment lies along this one, the other way round, as if sewn edge to edge. Start: its start meets this segment's end. End: its end meets this segment's start." => {
            "Esa pieza gira y se mueve para que su segmento quede a lo largo de este, en sentido contrario, como si se cosieran borde con borde. Inicio: su inicio se une al final de este segmento. Final: su final se une al inicio de este segmento."
        }
        "The end point moves; the start stays" => "Se mueve el punto final; el inicio se queda",
        "The start point moves; the end stays" => "Se mueve el punto inicial; el final se queda",
        "The two segments already have the same length" => {
            "Los dos segmentos ya tienen la misma longitud"
        }
        "Trims" => "Avíos",
        "Turn the image on the piece" => "Girar la imagen sobre la pieza",
        "Type the length this boundary must be, then set it exactly" => {
            "Escribe la longitud que debe tener este borde y ajústala con exactitud"
        }
        "Undo · Ctrl+Z" => "Deshacer · Ctrl+Z",
        "Unfolded into the whole piece" => "Desplegada en la pieza entera",
        "Unlock" => "Desbloquear",
        "Uses the two selected pieces' 2D overlap. Failed edits leave the document intact." => {
            "Usa el solape 2D de las dos piezas seleccionadas. Las ediciones fallidas dejan el documento intacto."
        }
        "Wait for the current step to finish, then try again" => {
            "Espera a que termine el paso en curso e inténtalo de nuevo"
        }
        "What goes wrong here? (optional)" => "¿Qué falla aquí? (opcional)",
        "1 · Pick first edge" => "1 · Elegir primer borde",
        "2 · CLICK THE SECOND EDGE" => "2 · HAZ CLIC EN EL SEGUNDO BORDE",
        "2 · Pick second edge" => "2 · Elegir segundo borde",
        "3 · CONFIRM SEAM" => "3 · CONFIRMAR COSTURA",
        "3 · Confirm seam" => "3 · Confirmar costura",
        "A stored target length is invalid and is ignored." => {
            "Una longitud objetivo guardada no es válida y se ignora."
        }
        "Active: simulated and colliding" => "Activa: simulada y con colisión",
        "Add to A" => "Añadir a A",
        "Add to B" => "Añadir a B",
        "Advanced meshing" => "Mallado avanzado",
        "Advanced stiffness" => "Rigidez avanzada",
        "Avatar collision diagnostics" => "Diagnóstico de colisión con el avatar",
        "BUILD THE TWO SEAM SIDES" => "CONSTRUYE LOS DOS LADOS DE LA COSTURA",
        "Bands" => "Bandas",
        "Bust" => "Busto",
        "Camera · standard views, projection and framing" => {
            "Cámara · vistas estándar, proyección y encuadre"
        }
        "Circle / ellipse" => "Círculo / elipse",
        "Click a segment" => "Haz clic en un segmento",
        "Click edges in sewing order. Enter switches A/B. Reverse changes one segment." => {
            "Haz clic en los bordes en orden de costura. Intro cambia A/B. Invertir cambia un segmento."
        }
        "Clockwise" => "Horario",
        "Counterclockwise" => "Antihorario",
        "Dart preview" => "Vista previa de la pinza",
        "Deactivated with its seams: held in place, not sewn, other cloth passes through it" => {
            "Desactivada con sus costuras: sujeta en su sitio, sin coser, otra tela la atraviesa"
        }
        "Deactivated: held in place, other cloth passes through it" => {
            "Desactivada: sujeta en su sitio, otra tela la atraviesa"
        }
        "Delete point (Del)" => "Borrar punto (Supr)",
        "Diagnostics" => "Diagnóstico",
        "Different lengths · click Sew with easing beside the cursor, or press E" => {
            "Longitudes distintas · haz clic en Coser con embebido junto al cursor o pulsa E"
        }
        "Different lengths: easing gathers the longer side. It does not soften the fabric." => {
            "Longitudes distintas: el embebido frunce el lado más largo. No ablanda el tejido."
        }
        "Dimensions and points" => "Medidas y puntos",
        "Display options" => "Opciones de visualización",
        "Drag an outline · release to preview · orange edge closes the stroke · Esc: cancel" => {
            "Arrastra un contorno · suelta para previsualizar · el borde naranja cierra el trazo · Esc: cancelar"
        }
        "EDIT SEAM" => "EDITAR COSTURA",
        "Ease" => "Holgura",
        "Edge start · 0 mm" => "Inicio del borde · 0 mm",
        "Expand" => "Ampliar",
        "Fabric" => "Tejido",
        "Files & formats" => "Archivos y formatos",
        "Frame all" => "Encuadrar todo",
        "Free patterns · Web" => "Patrones gratuitos · Web",
        "Global" => "Global",
        "Gradient" => "Degradado",
        "Hip" => "Cadera",
        "Hybrid VBD / OGC · frictionless · uncalibrated materials" => {
            "VBD / OGC híbrido · sin rozamiento · materiales sin calibrar"
        }
        "Hybrid VBD / OGC · lagged Coulomb friction · uncalibrated materials" => {
            "VBD / OGC híbrido · rozamiento de Coulomb diferido · materiales sin calibrar"
        }
        "Lengths differ. Adjust the ranges or explicitly allow easing." => {
            "Las longitudes difieren. Ajusta los tramos o permite el embebido de forma explícita."
        }
        "Mirrors and booleans" => "Simetrías y booleanas",
        "Motion / jitter" => "Movimiento / temblor",
        "No PNG images in this folder yet." => "Todavía no hay imágenes PNG en esta carpeta.",
        "No avatars (.obj, .gltf, .glb) in this folder yet." => {
            "Todavía no hay avatares (.obj, .gltf, .glb) en esta carpeta."
        }
        "No project files (.json) in this folder yet." => {
            "Todavía no hay archivos de proyecto (.json) en esta carpeta."
        }
        "PICK THE END OF THIS RANGE" => "ELIGE EL FINAL DE ESTE TRAMO",
        "Panels · Shift to add" => "Piezas · Mayús para añadir",
        "Piece" => "Pieza",
        "Points · Shift to add" => "Puntos · Mayús para añadir",
        "Ready · Space to start the simulation" => "Listo · Espacio para iniciar la simulación",
        "Rulers hidden while the board is rotated" => {
            "Reglas ocultas mientras el tablero está girado"
        }
        "SIMULATION PAUSED" => "SIMULACIÓN EN PAUSA",
        "Shapes" => "Formas",
        "Shoulder height" => "Altura de hombros",
        "Shoulders" => "Hombros",
        "Simulation" => "Simulación",
        "Sleeve" => "Manga",
        "Sleeve opening" => "Bocamanga",
        "Static OBJ, STL, PLY or COLLADA objects the cloth rests on or drapes over." => {
            "Objetos estáticos OBJ, STL, PLY o COLLADA sobre los que la tela se apoya o cae."
        }
        "Tint frozen and deactivated pieces" => "Teñir las piezas congeladas y desactivadas",
        "Turns about the bone's joint, around the world axes at rest; children follow." => {
            "Gira sobre la articulación del hueso, alrededor de los ejes del mundo en reposo; los hijos la siguen."
        }
        "Waist" => "Cintura",
        "centre" => "centro",
        "end" => "final",
        "longer" => "más largo",
        "shorter" => "más corto",
        "start" => "inicio",
        "Paint visible faces · Shift: erase · right drag: orbit · middle drag: pan · wheel: zoom" => {
            "Pinta caras visibles · Mayús: borrar · arrastre derecho: orbitar · arrastre central: desplazar · rueda: ampliar"
        }
        "G: move · R: rotate · X/Y/Z rings · Shift: 15° · Esc: cancel · right drag: orbit · middle drag: pan · wheel: zoom toward the cursor" => {
            "G: mover · R: girar · anillos X/Y/Z · Mayús: 15° · Esc: cancelar · arrastre derecho: orbitar · arrastre central: desplazar · rueda: ampliar hacia el cursor"
        }
        "Click boundaries to sew · right drag: orbit · middle drag: pan · wheel: zoom toward the cursor" => {
            "Haz clic en bordes para coser · arrastre derecho: orbitar · arrastre central: desplazar · rueda: ampliar hacia el cursor"
        }
        "Left: grab the fabric · right drag: orbit · middle drag: pan · wheel: zoom toward the cursor" => {
            "Izquierdo: agarrar el tejido · arrastre derecho: orbitar · arrastre central: desplazar · rueda: ampliar hacia el cursor"
        }
        "Graphics are moved on the 2D board · right drag: orbit · middle drag: pan · wheel: zoom toward the cursor" => {
            "Los gráficos se mueven en el tablero 2D · arrastre derecho: orbitar · arrastre central: desplazar · rueda: ampliar hacia el cursor"
        }
        "Select a boundary · right drag: orbit · middle drag: pan · wheel: zoom toward the cursor" => {
            "Selecciona un borde · arrastre derecho: orbitar · arrastre central: desplazar · rueda: ampliar hacia el cursor"
        }
        "Alt + left drag: rotate · middle or right drag: pan · wheel: zoom toward the cursor · [ and ]: rotate 15° · Home: reset view" => {
            "Alt + arrastre izquierdo: girar · arrastre central o derecho: desplazar · rueda: ampliar hacia el cursor · [ y ]: girar 15° · Inicio: restablecer vista"
        }
        "Box" => "Recuadro",
        "Clipboard" => "Portapapeles",
        "Copy panels" => "Copiar piezas",
        "Ctrl+C · copy panels and seams between them" => {
            "Ctrl+C · copia las piezas y las costuras entre ellas"
        }
        "Drag to select panels or contour points across multiple panels. Shift adds." => {
            "Arrastra para seleccionar piezas o puntos del contorno en varias piezas. Mayús añade."
        }
        "Draw a freehand selection on the board. Shift adds to the selection." => {
            "Dibuja a mano alzada una selección en el tablero. Mayús añade a la selección."
        }
        "Lasso" => "Lazo",
        "Lasso is too complex; draw a shorter, simpler shape" => {
            "El lazo es demasiado complejo; dibuja una forma más corta y sencilla"
        }
        "New arrangement · run to assemble the seams" => {
            "Nueva colocación · simula para montar las costuras"
        }
        "Panels" => "Piezas",
        "Panels pasted · internal seams retained · arrange copies in 3D before simulation · Ctrl+Z to undo" => {
            "Piezas pegadas · se conservan las costuras internas · coloca las copias en 3D antes de simular · Ctrl+Z para deshacer"
        }
        "Paste the last copied panels · Ctrl+V reads the system clipboard" => {
            "Pega las últimas piezas copiadas · Ctrl+V lee el portapapeles del sistema"
        }
        "Points" => "Puntos",
        "points" => "puntos",
        "panels" => "piezas",
        // Edits, as the history lists them.
        "Construct point" => "Construir punto",
        "Freehand piece" => "Pieza a mano alzada",
        "Lay out pieces" => "Distribuir piezas",
        "Add shape" => "Añadir forma",
        "Mirror" => "Simetría",
        "Linked mirror" => "Simetría enlazada",
        "Boolean" => "Booleana",
        "Internal line" => "Línea interna",
        "Remove line" => "Quitar línea",
        "Ruffle" => "Volante",
        "Draw piece" => "Dibujar pieza",
        "Place piece" => "Colocar pieza",
        "Arrange pieces" => "Colocar piezas",
        "Move pieces in 3D" => "Mover piezas en 3D",
        "Edit outline" => "Editar contorno",
        "Resize piece" => "Cambiar tamaño de la pieza",
        "Anchor piece" => "Anclar pieza",
        "Move points" => "Mover puntos",
        "Move point groups" => "Mover grupos de puntos",
        "Nudge" => "Desplazar",
        "Add point" => "Añadir punto",
        "Curve point" => "Punto de curva",
        "Target length" => "Longitud objetivo",
        "Seam allowance" => "Margen de costura",
        "Notch" => "Piquete",
        "Remove notch" => "Quitar piquete",
        "Move piece in 2D" => "Mover pieza en 2D",
        "Transform on the board" => "Transformar en el tablero",
        "Delete piece" => "Borrar pieza",
        "Delete pieces" => "Borrar piezas",
        "Archive pieces" => "Archivar piezas",
        "Cut and sew" => "Cortar y coser",
        "Replace outline" => "Reemplazar contorno",
        "Slash and spread" => "Abrir y separar",
        "Extend line" => "Prolongar línea",
        "Split along line" => "Dividir por la línea",
        "Unfold" => "Desplegar",
        "Merge along seam" => "Unir por la costura",
        "Restore archived" => "Recuperar archivadas",
        "Delete archived" => "Borrar archivadas",
        "Sew segments" => "Coser segmentos",
        "Remove seam" => "Quitar costura",
        "Fabric appearance" => "Apariencia del tejido",
        "Internal shape" => "Forma interna",
        "Cut hole" => "Recortar hueco",
        "Fill hole" => "Rellenar hueco",
        "Transform points" => "Transformar puntos",
        "Align points" => "Alinear puntos",
        "Align pieces" => "Alinear piezas",
        "Distribute pieces" => "Distribuir piezas",
        "Set length" => "Fijar longitud",
        "Match up" => "Emparejar",
        "Clone shape as piece" => "Clonar forma como pieza",
        "Clone outline as shape" => "Clonar contorno como forma",
        "Offset as lines" => "Desfase como líneas",
        "Perpendicular line" => "Línea perpendicular",
        "Clean up points" => "Limpiar puntos",
        "Group lines" => "Agrupar líneas",
        "Ungroup lines" => "Desagrupar líneas",
        "Optimize curve points" => "Optimizar puntos de curva",
        "Lock on the board" => "Bloquear en el tablero",
        "Flip faces" => "Invertir caras",
        "Seam fold" => "Pliegue de costura",
        "Line fold" => "Pliegue de línea",
        "Line elastic" => "Elástico de línea",
        "Shrink" => "Encogimiento",
        "Graphic" => "Gráfico",
        "Remove graphic" => "Quitar gráfico",
        "Trim" => "Avío",
        "Remove trim" => "Quitar avío",
        "Repeat trim" => "Repetir avío",
        "Unpin" => "Quitar alfiler",
        "Unsolidify" => "Quitar solidificar",
        "Load" => "Cargar",
        "Unfasten buttons" => "Desabrochar botones",
        "Start" => "Inicio",
        "edits are kept · pick one to go back to it" => {
            "ediciones se conservan · elige una para volver a ella"
        }
        "Undone · pick it to redo up to here" => "Deshecha · elígela para rehacer hasta aquí",
        "Shrink warp" => "Encogimiento urdimbre",
        "Shrink weft" => "Encogimiento trama",
        // Object browser.
        "Posed" => "En pose",
        "keys" => "claves",
        "Object" => "Objeto",
        "Garment" => "Prenda",
        "Fabrics" => "Tejidos",
        "Seams" => "Costuras",
        "Select the pieces cut from this fabric" => "Selecciona las piezas cortadas de este tejido",
        "Tacks" => "Hilvanes",
        "Internal lines with folds" => "Líneas internas con pliegue",
        "Elastic lines" => "Líneas elásticas",
        "Archived pieces" => "Piezas archivadas",
        "frozen" => "congelada",
        "deactivated" => "desactivada",
        "solidified" => "solidificada",
        "strengthened" => "reforzada",
        "locked in 2D" => "bloqueada en 2D",
        "flipped" => "invertida",
        "layer" => "capa",
        "pieces" => "piezas",
        "Drag a box to pin every visible point inside it" => {
            "Arrastra un recuadro para fijar cada punto visible dentro"
        }
        "Draw around the cloth to pin every visible point inside · Cmd+drag removes the pins inside" => {
            "Dibuja alrededor de la tela para fijar cada punto visible dentro · Cmd+arrastrar quita los alfileres de dentro"
        }
        "No visible cloth inside the lasso" => "No hay tela visible dentro del lazo",
        "selected · Shift-click: toggle · Shift-drag: add\nHold L + drag: lasso · Ctrl/Cmd: subtract · Alt: through\nMoved pieces keep their drape" => {
            "seleccionadas · Mayús+clic: alternar · Mayús+arrastrar: añadir\nMantén L + arrastrar: lazo · Ctrl/Cmd: restar · Alt: a través\nLas piezas movidas conservan su drapeado"
        }
        "Click the cloth to pin a point where it is · drag a box to pin an area · click a pin to remove it · Cmd+drag removes the pins in a box · drag a pin to move its pinned area" => {
            "Haz clic en la tela para fijar un punto donde está · arrastra un recuadro para fijar una zona · haz clic en un alfiler para quitarlo · Cmd+arrastrar quita los alfileres de un recuadro · arrastra un alfiler para mover su zona fijada"
        }
        "moved · the cloth follows them when simulated" => {
            "movidos · la tela los sigue al simular"
        }
        "Move pins" => "Mover alfileres",
        "Skirt from measurements…" => "Falda por medidas…",
        "Skirt from measurements" => "Falda por medidas",
        "Hip depth" => "Profundidad de cadera",
        "Flare" => "Vuelo",
        "Waist height" => "Altura de cintura",
        "Waist, hip, depths, length and placement from the current avatar" => {
            "Cintura, cadera, profundidades, largo y colocación del avatar actual"
        }
        "Replace the garment with this skirt" => "Sustituye la prenda por esta falda",
        "Skirt drafted from the measurements" => "Falda trazada a partir de las medidas",
        "Trousers from measurements…" => "Pantalón por medidas…",
        "Trousers from measurements" => "Pantalón por medidas",
        "Rise" => "Tiro",
        "Leg opening" => "Bajo de pierna",
        "Waist, hip, depths, rise, length and placement from the current avatar" => {
            "Cintura, cadera, profundidades, tiro, largo y colocación del avatar actual"
        }
        "Replace the garment with these trousers" => "Sustituye la prenda por este pantalón",
        "Trousers drafted from the measurements" => "Pantalón trazado a partir de las medidas",
        "Arm length" => "Largo de brazo",
        "Inseam" => "Entrepierna",
        "The legs do not part below the pelvis" => "Las piernas no se separan bajo la pelvis",
        // Command palette.
        "Start or pause simulation" => "Iniciar o pausar la simulación",
        "Advance one simulation step" => "Avanzar un paso de simulación",
        "Reset flat arrangement" => "Restablecer la colocación plana",
        "Export session artifacts" => "Exportar los archivos de la sesión",
        "Toggle properties inspector" => "Mostrar u ocultar el inspector de propiedades",
        "Reset workspace layout" => "Restablecer la distribución del espacio de trabajo",
        "Open preferences" => "Abrir preferencias",
        "Load T-shirt example" => "Cargar el ejemplo de camiseta",
        "Load dress example" => "Cargar el ejemplo de vestido",
        "Select and edit" => "Seleccionar y editar",
        "Draw pattern" => "Dibujar patrón",
        "Draw freehand pattern" => "Dibujar patrón a mano alzada",
        "Draw rectangle" => "Dibujar rectángulo",
        "Draw ellipse" => "Dibujar elipse",
        "Draw arc sector" => "Dibujar sector de arco",
        "Draw internal line" => "Dibujar línea interna",
        "Sew boundaries" => "Coser bordes",
        "Move panel in 3D" => "Mover pieza en 3D",
        "Rotate panel in 3D" => "Girar pieza en 3D",
        "Measure on 2D board" => "Medir en el tablero 2D",
        "Lock selected pieces in 2D" => "Bloquear en 2D las piezas seleccionadas",
        "Lock all pieces in 2D" => "Bloquear en 2D todas las piezas",
        "Unlock all pieces in 2D" => "Desbloquear en 2D todas las piezas",
        "Lock all internal lines in 2D" => "Bloquear en 2D todas las líneas internas",
        "Unlock all internal lines in 2D" => "Desbloquear en 2D todas las líneas internas",
        "Invert piece selection" => "Invertir la selección de piezas",
        "Reset 3D arrangement of selected pieces" => {
            "Restablecer la colocación 3D de las piezas seleccionadas"
        }
        "Save project as…" => "Guardar proyecto como…",
        "Import avatar…" => "Importar avatar…",
        "Import DXF patterns…" => "Importar patrones DXF…",
        "Import SVG patterns…" => "Importar patrones SVG…",
        "Front view" => "Vista frontal",
        "Back view" => "Vista trasera",
        "View of the avatar's right side" => "Vista del lado derecho del avatar",
        "View of the avatar's left side" => "Vista del lado izquierdo del avatar",
        "Top view" => "Vista superior",
        "Bottom view" => "Vista inferior",
        "Three-quarter view, avatar's right" => "Vista tres cuartos, derecha del avatar",
        "Three-quarter view, avatar's left" => "Vista tres cuartos, izquierda del avatar",
        "Select all pieces" => "Seleccionar todas las piezas",
        "Copy pieces" => "Copiar piezas",
        "Paste pieces" => "Pegar piezas",
        "Delete selected pieces" => "Borrar las piezas seleccionadas",
        "Deactivate pieces" => "Desactivar piezas",
        "Deactivate pieces and their seams" => "Desactivar piezas y sus costuras",
        "Activate pieces" => "Activar piezas",
        "Freeze pieces" => "Congelar piezas",
        "Unfreeze pieces" => "Descongelar piezas",
        "Strengthen pieces" => "Reforzar piezas",
        "Solidify pieces" => "Solidificar piezas",
        "Flip piece normals" => "Invertir las caras de las piezas",
        "Add a button" => "Añadir un botón",
        "Add a buttonhole" => "Añadir un ojal",
        "Add a graphic…" => "Añadir un gráfico…",
        "Cut and sew along the selected line" => "Cortar y coser por la línea seleccionada",
        "Replace the outline with the selected shape" => {
            "Reemplazar el contorno por la forma seleccionada"
        }
        "Slash and spread along the selected line" => "Abrir y separar por la línea seleccionada",
        "Merge the pieces of the selected seam" => "Unir las piezas de la costura seleccionada",
        "Unfold the piece across the selected segment" => {
            "Desplegar la pieza sobre el segmento seleccionado"
        }
        "Extend the selected line to the outline" => {
            "Prolongar la línea seleccionada hasta el contorno"
        }
        "Add a diamond dart to the selected piece" => {
            "Añadir una pinza en rombo a la pieza seleccionada"
        }
        "Avatar pose…" => "Pose del avatar…",
        "Show the avatar's skeleton" => "Mostrar el esqueleto del avatar",
        "Put the avatar at rest" => "Poner el avatar en reposo",
        "Delete the scene objects" => "Borrar los objetos de la escena",
        "Play or pause the animation" => "Reproducir o pausar la animación",
        "Record the cloth along the animation" => "Grabar la tela a lo largo de la animación",
        "Delete the animation" => "Borrar la animación",
        "Export the take as a point cache…" => "Exportar la toma como caché de puntos…",
        "Export the take as an OBJ sequence…" => "Exportar la toma como secuencia OBJ…",
        "Save the 3D view as PNG…" => "Guardar la vista 3D como PNG…",
        "Capture the take as a video…" => "Capturar la toma como vídeo…",
        "Export glTF / GLB…" => "Exportar glTF / GLB…",
        "Export OBJ…" => "Exportar OBJ…",
        "Export the selected pieces as OBJ…" => "Exportar las piezas seleccionadas como OBJ…",
        "Export the take as an MDD cache…" => "Exportar la toma como caché MDD…",
        "Export the take as an MDD cache for Maya or 3ds Max…" => {
            "Exportar la toma como caché MDD para Maya o 3ds Max…"
        }
        "Export USD / USDZ…" => "Exportar USD / USDZ…",
        "Export FBX…" => "Exportar FBX…",
        "Commands" => "Órdenes",
        "Search actions · arrow keys / Page Up Down navigate · Enter run · Esc close" => {
            "Busca acciones · flechas / Re Pág Av Pág para moverse · Intro ejecuta · Esc cierra"
        }
        "Search commands…" => "Buscar órdenes…",
        "No matching commands" => "Ninguna orden coincide",
        "Close" => "Cerrar",
        "No garment document is loaded" => "No hay ningún documento de prenda cargado",
        "Select a panel first" => "Selecciona antes una pieza",
        "The simulation worker is unavailable" => "El proceso de simulación no está disponible",
        "Local export storage is unavailable" => {
            "El almacenamiento local de exportación no está disponible"
        }
        "Pause simulation before stepping" => "Pausa la simulación antes de avanzar paso a paso",
        "The previous simulation step is still pending" => {
            "El paso de simulación anterior aún no ha terminado"
        }
        // Fabric tab.
        "A different colour on the fabric's back face, as on a lining or a two-tone fabric" => {
            "Un color distinto en el revés del tejido, como en un forro o un tejido de dos tonos"
        }
        "Area weight" => "Gramaje",
        "Back color" => "Color del revés",
        "Bending warp" => "Flexión urdimbre",
        "Bending weft" => "Flexión trama",
        "Choose a fabric type" => "Elige un tipo de tejido",
        "Color (sRGB)" => "Color (sRGB)",
        "Custom color and roughness" => "Color y rugosidad propios",
        "Draw the cloth this thick, with a rim along its outline; the simulation is unchanged" => {
            "Dibuja la tela con este grosor y un canto en su contorno; la simulación no cambia"
        }
        "Eyedropper: click a piece in the 3D view to take its colour" => {
            "Cuentagotas: haz clic en una pieza de la vista 3D para tomar su color"
        }
        "Image…" => "Imagen…",
        "Internal damping" => "Amortiguación interna",
        "Knit" => "Punto",
        "Look thickness" => "Grosor visual",
        "Measurement source" => "Origen de la medida",
        "None" => "Ninguno",
        "Pick" => "Tomar",
        "Plain" => "Tafetán",
        "Repeat" => "Repetición",
        "Repeat a PNG image over the fabric along its grain" => {
            "Repite una imagen PNG sobre el tejido siguiendo el hilo"
        }
        "Roughness" => "Rugosidad",
        "Satin" => "Raso",
        "Shared by panels using this fabric. Does not change its mechanics." => {
            "Común a las piezas de este tejido. No cambia su mecánica."
        }
        "Shear" => "Cizalla",
        "Thickness" => "Grosor",
        "Thread spacing" => "Separación de hilos",
        "Twill" => "Sarga",
        "Warp" => "Urdimbre",
        "Warp/weft membrane" => "Membrana urdimbre/trama",
        "Weave" => "Ligamento",
        "Weave contrast" => "Contraste del ligamento",
        "Weft" => "Trama",
        "image" => "imagen",
        "sRGB byte (0-255). Apply appearance to commit." => {
            "Byte sRGB (0-255). Aplicar apariencia para confirmarlo."
        }
        "Apply appearance" => "Aplicar apariencia",
        "Mass: panel" => "Masa: pieza",
        "garment" => "prenda",
        "Search actions · Ctrl/Cmd+Shift+P" => "Buscar acciones · Ctrl/Cmd+Mayús+P",
        // Status messages set outside the editor windows.
        "Dynamic simulation · OGC contacts" => "Simulación dinámica · contactos OGC",
        "Draft metadata updated" => "Datos del borrador actualizados",
        "Edit applied · edited pieces keep their 3D shape · ready to simulate" => {
            "Edición aplicada · las piezas editadas conservan su forma 3D · lista para simular"
        }
        "Edit applied · the edited piece starts flat again: keeping its 3D shape would start inside the avatar or itself" => {
            "Edición aplicada · la pieza editada vuelve a empezar plana: conservar su forma 3D la haría empezar dentro del avatar o de sí misma"
        }
        "Edit applied · unchanged panels retained · ready to simulate" => {
            "Edición aplicada · se conservan las piezas sin cambios · lista para simular"
        }
        "2D layout changed · 3D state preserved" => {
            "Distribución 2D cambiada · estado 3D conservado"
        }
        "Avatar replaced · drape reset · save to keep the imported asset" => {
            "Avatar sustituido · drapeado reiniciado · guarda para conservar el recurso importado"
        }
        "My poses" => "Mis poses",
        "Use this pose · right-click to delete it" => {
            "Usar esta pose · clic derecho para borrarla"
        }
        "Pose name" => "Nombre de la pose",
        "Save pose" => "Guardar pose",
        "Keep this pose under its name for every project" => {
            "Guarda esta pose con su nombre para todos los proyectos"
        }
        "Arms down" => "Brazos abajo",
        "T-pose" => "Pose en T",
        "Arms forward" => "Brazos al frente",
        "Step" => "Paso",
        "Key at" => "Clave en",
        "Keep this position and turn at the animation's current time; while recording, the object moves from key to key" => {
            "Guarda esta posición y giro en el instante actual de la animación; al grabar, el objeto va de clave en clave"
        }
        "Clear keys" => "Borrar claves",
        "Keys" => "Claves",
        "OBJ, STL, PLY or COLLADA objects the cloth rests on or drapes over; with keys they move while the animation records." => {
            "Objetos OBJ, STL, PLY o COLLADA sobre los que la tela se apoya o cae; con claves se mueven mientras se graba la animación."
        }
        "Key wind" => "Clave de viento",
        "Key gravity" => "Clave de gravedad",
        "Key the scene's wind, as set in Simulation, at this time; recording blows it from key to key" => {
            "Pone como clave en este instante el viento de la escena, tal como está en Simulación; al grabar sopla de clave en clave"
        }
        "Key the scene's gravity at this time; recording changes it from key to key" => {
            "Pone como clave en este instante la gravedad de la escena; al grabar cambia de clave en clave"
        }
        "Wind and gravity keys" => "Claves de viento y gravedad",
        "Import a BVH motion…" => "Importar un movimiento BVH…",
        "Motion (BVH)…" => "Movimiento (BVH)…",
        "Relief (mm)" => "Relieve (mm)",
        "Bright parts of the image stand out by this much; 0 is flat" => {
            "Las partes claras de la imagen sobresalen esto; 0 es plano"
        }
        "The colours tint the image: white shows it as it is." => {
            "Los colores tiñen la imagen: el blanco la muestra tal cual."
        }
        "Normal map" => "Mapa de normales",
        "Roughness map" => "Mapa de rugosidad",
        "An image of the surface's normals over one repeat: red along it, green up it, blue out of the cloth" => {
            "Una imagen de las normales de la superficie en una repetición: rojo a lo largo, verde hacia arriba, azul hacia fuera de la tela"
        }
        "A grey image of the roughness over one repeat: black glossy, white matt" => {
            "Una imagen gris de la rugosidad en una repetición: negro brillante, blanco mate"
        }
        "Map staged · Apply appearance to show it" => {
            "Mapa preparado · Aplicar apariencia para mostrarlo"
        }
        "Attach all pins to the avatar" => "Sujetar todos los alfileres al avatar",
        "Detach all pins from the avatar" => "Soltar todos los alfileres del avatar",
        "Pins attached to the avatar · they follow it while an animation records" => {
            "Alfileres sujetos al avatar · lo siguen mientras se graba una animación"
        }
        "Pins detached from the avatar" => "Alfileres soltados del avatar",
        "Attach pins to the avatar" => "Sujetar alfileres al avatar",
        "Detach pins from the avatar" => "Soltar alfileres del avatar",
        "There are no pins to attach; pin points with the Pin tool first" => {
            "No hay alfileres que sujetar; fija antes puntos con la herramienta Alfiler"
        }
        "Move joints" => "Mover articulaciones",
        "Drag the skeleton's joints to fit the body; Apply and restart uses them" => {
            "Arrastra las articulaciones del esqueleto para ajustarlo al cuerpo; Aplicar y reiniciar las usa"
        }
        "Put the avatar at rest to move its joints" => {
            "Pon el avatar en reposo para mover sus articulaciones"
        }
        "joint moved · Apply and restart to use the new skeleton" => {
            "articulación movida · Aplicar y reiniciar para usar el esqueleto nuevo"
        }
        _ => return None,
    })
}

/// "Menu coverage" line of the Help menu.
pub(super) fn coverage_line(language: Language, done: usize, all: usize) -> String {
    let percent = done * 100 / all.max(1);
    match language {
        Language::English => format!("Menu coverage: {done} of {all} items work ({percent} %)"),
        Language::Spanish => {
            format!("Cobertura del menú: funcionan {done} de {all} elementos ({percent} %)")
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn status_messages_with_values_are_translated_around_them() {
        use super::{Language, status};
        assert_eq!(
            status(Language::Spanish, "Capturing the take… frame 3 of 40"),
            "Capturando la toma… fotograma 3 de 40"
        );
        assert_eq!(
            status(
                Language::Spanish,
                "144 pins added · the points stay where they are"
            ),
            "144 alfileres añadidos · los puntos se quedan donde están"
        );
        assert_eq!(
            status(
                Language::Spanish,
                "Exported a.glb: 10 vertices, 4 faces, 0 textures"
            ),
            "Exportado a.glb: 10 vértices, 4 caras, 0 texturas"
        );
        // Exact messages first; unknown ones and English stay as they are.
        assert_eq!(status(Language::Spanish, "Pressure"), "Presión");
        assert_eq!(
            status(
                Language::Spanish,
                "Graphic logo lies outside the new outline; move it first"
            ),
            "El gráfico logo queda fuera del contorno nuevo; muévelo antes"
        );
        assert_eq!(status(Language::Spanish, "Something new"), "Something new");
        assert_eq!(
            status(Language::English, "Capturing the take… frame 3 of 40"),
            "Capturing the take… frame 3 of 40"
        );
        // Every template has a translation with as many values.
        for (template, translation) in super::STATUS_TEMPLATES {
            assert_eq!(
                template.matches("{}").count(),
                translation.matches("{}").count(),
                "{template}"
            );
            assert!(!template.contains("{}{}"), "{template}");
        }
    }

    use super::super::menus::{Item, MENUS};
    use super::*;

    fn labels(items: &[Item], out: &mut Vec<&'static str>) {
        for item in items {
            match *item {
                Item::Run(label, _) | Item::Todo(label) => out.push(label),
                Item::Sub(label, inner) => {
                    out.push(label);
                    labels(inner, out);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn every_menu_label_has_a_spanish_name() {
        let mut all = vec![];
        for (title, items) in MENUS {
            all.push(*title);
            labels(items, &mut all);
        }
        let missing: Vec<_> = all.iter().filter(|l| spanish(l).is_none()).collect();
        assert!(missing.is_empty(), "no Spanish for {missing:?}");
        assert_eq!(tr(Language::Spanish, "Freeze"), "Congelar");
        assert_eq!(tr(Language::English, "Freeze"), "Freeze");
        assert_eq!(tr(Language::Spanish, "Something new"), "Something new");
    }
}
