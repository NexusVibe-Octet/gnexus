//! The few texts drawn by the Rust side (tray menu, file dialogs), in the
//! interface language. The page tells which language it uses
//! (`ui_language`); everything else is translated in `ui/i18n.js`.
//! Error messages stay in French.

/// French text → [en, de, it, es].
const TR: &[(&str, [&str; 4])] = &[
    ("Ouvrir G Nexus", ["Open G Nexus", "G Nexus öffnen", "Apri G Nexus", "Abrir G Nexus"]),
    ("Quitter", ["Quit", "Beenden", "Esci", "Salir"]),
    (
        "Exporter la configuration de G Nexus",
        [
            "Export the G Nexus configuration",
            "G-Nexus-Konfiguration exportieren",
            "Esporta la configurazione di G Nexus",
            "Exportar la configuración de G Nexus",
        ],
    ),
    (
        "Importer une configuration G Nexus",
        [
            "Import a G Nexus configuration",
            "G-Nexus-Konfiguration importieren",
            "Importa una configurazione di G Nexus",
            "Importar una configuración de G Nexus",
        ],
    ),
    (
        "Sauvegarde G Nexus",
        ["G Nexus backup", "G-Nexus-Sicherung", "Backup di G Nexus", "Copia de seguridad de G Nexus"],
    ),
    (
        "Choisir l'exécutable du jeu ou de l'application",
        [
            "Choose the game or application executable",
            "Programmdatei des Spiels oder der Anwendung wählen",
            "Scegli l'eseguibile del gioco o dell'applicazione",
            "Elegir el ejecutable del juego o de la aplicación",
        ],
    ),
    ("Programmes", ["Programs", "Programme", "Programmi", "Programas"]),
    (
        "Exporter un profil d'égaliseur",
        ["Export an equalizer profile", "Equalizer-Profil exportieren", "Esporta un profilo dell'equalizzatore", "Exportar un perfil de ecualizador"],
    ),
    (
        "Importer un profil d'égaliseur",
        ["Import an equalizer profile", "Equalizer-Profil importieren", "Importa un profilo dell'equalizzatore", "Importar un perfil de ecualizador"],
    ),
    ("Profil d'égaliseur", ["Equalizer profile", "Equalizer-Profil", "Profilo dell'equalizzatore", "Perfil de ecualizador"]),
    (
        "Exporter un profil de souris",
        ["Export a mouse profile", "Mausprofil exportieren", "Esporta un profilo del mouse", "Exportar un perfil de ratón"],
    ),
    (
        "Importer un profil de souris",
        ["Import a mouse profile", "Mausprofil importieren", "Importa un profilo del mouse", "Importar un perfil de ratón"],
    ),
    (
        "Profil de souris G Nexus",
        ["G Nexus mouse profile", "G-Nexus-Mausprofil", "Profilo mouse G Nexus", "Perfil de ratón de G Nexus"],
    ),
    (
        "Importer un profil Logitech Gaming Software",
        [
            "Import a Logitech Gaming Software profile",
            "Logitech-Gaming-Software-Profil importieren",
            "Importa un profilo Logitech Gaming Software",
            "Importar un perfil de Logitech Gaming Software",
        ],
    ),
    (
        "Exporter vers Logitech Gaming Software",
        [
            "Export to Logitech Gaming Software",
            "Für Logitech Gaming Software exportieren",
            "Esporta per Logitech Gaming Software",
            "Exportar a Logitech Gaming Software",
        ],
    ),
    (
        "Profil Logitech Gaming Software (XML)",
        [
            "Logitech Gaming Software profile (XML)",
            "Logitech-Gaming-Software-Profil (XML)",
            "Profilo Logitech Gaming Software (XML)",
            "Perfil de Logitech Gaming Software (XML)",
        ],
    ),
];

pub const LANGS: [&str; 5] = ["fr", "en", "de", "it", "es"];

/// `text` (French) in `lang`; French when unknown.
pub fn tr(lang: &str, text: &'static str) -> &'static str {
    let Some(i) = LANGS.iter().position(|l| *l == lang).filter(|&i| i > 0) else { return text };
    TR.iter().find(|(fr, _)| *fr == text).map_or(text, |(_, t)| t[i - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_french() {
        assert_eq!(tr("de", "Quitter"), "Beenden");
        assert_eq!(tr("fr", "Quitter"), "Quitter");
        assert_eq!(tr("xx", "Quitter"), "Quitter");
        assert_eq!(tr("en", "pas traduit"), "pas traduit");
    }
}
