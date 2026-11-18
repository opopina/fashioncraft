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
