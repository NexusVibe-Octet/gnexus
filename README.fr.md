# G Nexus

*[English version](README.md)*

G Nexus remplace Logitech Gaming Software et G HUB pour trois appareils Logitech, sous **Windows** et **Linux** :

- clavier **G13 Advanced Gameboard**
- souris **G502 X PLUS** (filaire ou récepteur LIGHTSPEED)
- casque **PRO X Wireless**

Il tourne dans la zone de notification, sans logiciel Logitech, et fonctionne sans connexion internet.

[![Buy Me a Coffee at ko-fi.com](https://storage.ko-fi.com/cdn/kofi6.png?v=6)](https://ko-fi.com/W4F728ZFSH)

## Fonctionnalités

**G13**
- Toutes les touches réaffectables : touches et raccourcis, boutons et molette de souris, macros, touches multi-états (appui court / long / double)
- Trois banques (M1 à M3) avec leur couleur de rétroéclairage, profils par application (changés automatiquement selon le jeu au premier plan)
- Macros : éditeur, bibliothèque, enregistrement avec la touche RECORD (MR)
- Stick en quatre touches, ou joystick analogique d'une manette Xbox 360 virtuelle
- Applets de l'écran LCD : horloge, moniteur système, lecture en cours (commandes média), choix du profil
- Overlay en jeu pour modifier les touches sans quitter le jeu
- Import / export des profils Logitech Gaming Software (`.xml`)

**G502 X PLUS**
- Paliers DPI, fréquence, éclairage
- Profils intégrés à la souris, ou mode temps réel avec un profil par jeu
- Réaffectation des boutons et macros souris

**PRO X Wireless**
- Batterie, retour micro, mise en veille automatique, volume
- Égaliseur 10 bandes propre à G Nexus, avec des profils par application

**Et aussi** : sauvegardes automatiques, export / import de toute la configuration (`.g13backup`), interface en français, anglais, allemand, italien et espagnol.

## Téléchargement

Les installateurs sont joints à chaque [version publiée](../../releases) : installateur Windows et paquet Arch / CachyOS.

### Windows

1. Lancez l'installateur (`G Nexus_x.y.z_x64-setup.exe`). WebView2 est nécessaire (présent dans Windows 11).
2. Fermez Logitech Gaming Software : il capte les touches du G13. Gardez G HUB fermé pendant que G Nexus pilote la souris ou le casque.
3. Facultatif : le pilote [ViGEmBus](https://github.com/nefarius/ViGEmBus) pour le mode manette du stick.
4. L'égaliseur s'installe depuis la page Casque (Windows demande une fois les droits administrateur).

### Linux

Testé sous CachyOS (Arch), KDE Plasma 6, Wayland, PipeWire. Le changement automatique de profil et l'overlay en jeu demandent **KDE Plasma** ; tout le reste fonctionne sur n'importe quel bureau.

1. Arch / CachyOS : installez le paquet joint à la version publiée, `sudo pacman -U gnexus-x.y.z-1-x86_64.pkg.tar.zst` (ou construisez-le avec le [`PKGBUILD`](deploy/linux/arch)). Il installe les règles udev et charge `uinput` ; rebranchez ensuite les appareils. Autres distributions : compilez depuis les sources (voir plus bas).
2. L'accès aux appareils sans droits root vient des règles udev de [`deploy/linux`](deploy/linux) (installées par le paquet). La règle facultative `71-gnexus-record.rules` ne sert qu'à enregistrer des macros : le paquet indique comment l'activer.
3. Ne faites pas tourner en même temps un autre programme qui pilote les mêmes appareils (Solaar, Piper / libratbag, HeadsetControl).
4. L'égaliseur est un filtre PipeWire placé devant la sortie du casque : aucun droit root, et la sortie par défaut ne change pas.

Votre configuration Windows se transfère : *Paramètres › Configuration › Exporter* sous Windows, puis *Importer* sous Linux.

## Compiler depuis les sources

Rust (stable) est nécessaire. Tests : `cargo test --workspace` ; lint : `cargo clippy --workspace --all-targets`.

- **Linux** (paquets Arch) : `webkit2gtk-4.1 gtk3 libayatana-appindicator hidapi systemd-libs base-devel`, puis
  `CARGO_TARGET_DIR=target cargo build --release -p g13-gui` (binaire `target/release/g13-studio`).
  `.cargo/config.toml` contient un dossier de compilation propre à la machine Windows de l'auteur : `CARGO_TARGET_DIR` le remplace.
- **Windows** (toolchain GNU) : depuis `crates/g13-gui`, `npx @tauri-apps/cli@2 build` produit l'installateur NSIS.

Documentation technique : [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/LINUX_PORT.md`](docs/LINUX_PORT.md).

## Crédits

Design inspiré par la chaîne YouTube [JaspMakes](https://www.youtube.com/watch?v=315USgAl2Ks).

## Avertissement

G Nexus n'est pas affilié à Logitech. Logitech, G13, G502 et PRO X sont des marques de Logitech.
Il dialogue avec les appareils par leurs propres protocoles (HID, HID++ 2.0) ; utilisation à vos risques.

## Licence

Au choix : [licence Apache 2.0](LICENSE-APACHE) ou [licence MIT](LICENSE-MIT).
