# Logitech G13 Advanced Gameboard - Spécification Technique du Protocole USB/HID

Document de référence officiel pour l'implémentation d'un pilote moderne multiplateforme (Rust).  
Ce document détaille l'ensemble de la communication USB/HID entre l'hôte et le périphérique physique Logitech G13.

---

## 1. Caractéristiques Matérielles et Identifiants USB

| Paramètre | Valeur | Description |
| :--- | :--- | :--- |
| **Nom Commercial** | Logitech G13 Advanced Gameboard | Clavier ergonomique hybride pour joueur |
| **Vendor ID (VID)** | `0x046D` | Logitech Inc. |
| **Product ID (PID)** | `0xC21C` | G13 Gameboard |
| **Release USB (bcdDevice)** | `0x0203` | Version firmware 2.03 |
| **Classe USB** | `0x03` (HID) | Human Interface Device |
| **Interface** | Interface `0` | Interface composite unique avec Endpoints Interrupt IN/OUT |
| **Usage Page** | `0xFF00` | Vendor-Defined Page 1 |
| **Usage** | `0x0000` | Generic Vendor Usage |
| **Touches Physiques** | 29 boutons | 22 touches principales (`G1`..`G22`), 2 touches pouce (`G23`, `G24`), 1 clic stick (`G25`), 4 touches profil (`M1`..`M3`, `MR`) |
| **Touches LCD** | 5 boutons | 4 boutons de contrôle LCD (`L1`..`L4`) + 1 bouton rétroéclairage (`BD`) |
| **Mini-Stick Analogique** | 2 axes (X, Y) | Potentiomètre analogique 8-bit par axe |
| **Écran LCD** | 160 × 43 pixels | Écran graphique monochrome à contraste STN |
| **Rétroéclairage** | RGB (24-bit) | LED RGB pilotables individuellement en intensité R, G, B |
| **LEDs d'état M-Keys** | 4 LEDs | Indicateurs dédiés pour M1, M2, M3, MR |
| **Mémoire Embarquée** | 256 octets | Mémoire non-volatile pour profils nomades (*Profiles To Go*) |

---

## 2. Descripteur de Rapport HID (HID Report Descriptor)

Le microprogramme du G13 expose un descripteur HID compact de **100 octets** sous l'Usage Page propriétaire `0xFF00`.

### 2.1 Dump Hexadécimal Brut (100 octets)
```
06 00 FF 09 00 A1 01 85 01 09 01 15 00 26 FF 00 75 08 95 07 81 02
85 03 09 02 15 00 26 FF 00 75 08 96 DF 03 91 02
85 07 09 03 15 00 26 FF 00 75 08 95 04 B1 02
85 04 09 04 15 00 26 FF 00 75 08 95 04 B1 02
85 05 09 05 15 00 26 FF 00 75 08 95 04 B1 02
85 06 09 06 15 00 26 FF 00 75 08 96 01 01 B1 02
C0
```

### 2.2 Décodage Annoté des Éléments HID

```
Item HID                     Valeur          Signification
--------------------------------------------------------------------------------------
Usage Page (Vendor 0xFF00)   06 00 FF        Espace d'usage propriétaire
Usage (0x00)                 09 00           Usage générique G13
Collection (Application)     A1 01           Début de la collection principale

  -- Rapport d'Entrée 0x01 (Touches & Mini-Stick) --
  Report ID (1)              85 01           Identifiant Report ID 0x01
  Usage (0x01)               09 01           Usage données d'entrée
  Logical Minimum (0)        15 00           Valeur logique min = 0
  Logical Maximum (255)      26 FF 00        Valeur logique max = 255
  Report Size (8)            75 08           8 bits par unité
  Report Count (7)           95 07           7 octets de données utiles
  Input (Data, Var, Abs)     81 02           Rapport d'interruption entrant (7 octets)

  -- Rapport de Sortie 0x03 (Affichage Écran LCD) --
  Report ID (3)              85 03           Identifiant Report ID 0x03
  Usage (0x02)               09 02           Usage données LCD
  Logical Minimum (0)        15 00           Valeur logique min = 0
  Logical Maximum (255)      26 FF 00        Valeur logique max = 255
  Report Size (8)            75 08           8 bits par unité
  Report Count (991)         96 DF 03        0x03DF = 991 octets de données utiles
  Output (Data, Var, Abs)    91 02           Rapport de sortie (1 ID + 991 octets = 992 octets)

  -- Feature Report 0x07 (Rétroéclairage RGB) --
  Report ID (7)              85 07           Identifiant Report ID 0x07
  Usage (0x03)               09 03           Usage rétroéclairage
  Logical Minimum (0)        15 00           0
  Logical Maximum (255)      26 FF 00        255
  Report Size (8)            75 08           8 bits
  Report Count (4)           95 04           4 octets de données utiles
  Feature (Data, Var, Abs)   B1 02           Rapport de configuration (1 ID + 4 octets = 5 octets)

  -- Feature Report 0x04 (Configuration / Mode) --
  Report ID (4)              85 04           Identifiant Report ID 0x04
  Usage (0x04)               09 04           Usage configuration
  Report Count (4)           95 04           4 octets utiles (5 octets au total)
  Feature (Data, Var, Abs)   B1 02

  -- Feature Report 0x05 (LEDs d'indication M1..MR) --
  Report ID (5)              85 05           Identifiant Report ID 0x05
  Usage (0x05)               09 05           Usage LED de profils
  Report Count (4)           95 04           4 octets utiles (5 octets au total)
  Feature (Data, Var, Abs)   B1 02

  -- Feature Report 0x06 (Mémoire Embarquée / Profils Nomades) --
  Report ID (6)              85 06           Identifiant Report ID 0x06
  Usage (0x06)               09 06           Usage mémoire flash/EEPROM
  Report Count (257)         96 01 01        0x0101 = 257 octets utiles (258 octets au total)
  Feature (Data, Var, Abs)   B1 02

End Collection               C0              Fin de la collection
```

---

## 3. Protocole d'Entrée : Report ID `0x01` (Périphérique -> Hôte)

Le paquet d'entrée mesure **8 octets** au total (1 octet de Report ID suivi de 7 octets de données). Il est transmis sur l'endpoint d'interruption `IN` dès qu'un état change, ou interrogeable via `HidD_GetInputReport`.

### 3.1 Vue d'ensemble de la Trame (8 octets)

```
+---------+---------+---------+---------+---------+---------+---------+---------+
| Octet 0 | Octet 1 | Octet 2 | Octet 3 | Octet 4 | Octet 5 | Octet 6 | Octet 7 |
+---------+---------+---------+---------+---------+---------+---------+---------+
|   0x01  | Stick X | Stick Y | G1..G8  | G9..G16 | G17..G22| BD..M3  | MR..G25 |
+---------+---------+---------+---------+---------+---------+---------+---------+
```

### 3.2 Détail Octet par Octet

#### Octet 0 : Identifiant du Rapport
* Valeur constante : `0x01`.

#### Octets 1 & 2 : Mini-Stick Analogique
* **Octet 1 (Axe X) :** Valeur non signée `0..255`.
  * `0x00` : Pleine course à Gauche
  * `~0x7C` à `0x80` : Position centrale de repos
  * `0xFF` : Pleine course à Droite
* **Octet 2 (Axe Y) :** Valeur non signée `0..255`.
  * `0x00` : Pleine course en Haut
  * `~0x6F` à `0x80` : Position centrale de repos
  * `0xFF` : Pleine course en Bas

#### Octet 3 : Touches G1 à G8 (Bitmask, actif à `1`)
| Bit | Masque | Touche Associée | Emplacement Physique |
| :---: | :---: | :---: | :--- |
| **0** | `0x01` | **G1** | Rangée supérieure, touche 1 (gauche) |
| **1** | `0x02` | **G2** | Rangée supérieure, touche 2 |
| **2** | `0x04` | **G3** | Rangée supérieure, touche 3 |
| **3** | `0x08` | **G4** | Rangée supérieure, touche 4 |
| **4** | `0x10` | **G5** | Rangée supérieure, touche 5 |
| **5** | `0x20` | **G6** | Rangée supérieure, touche 6 |
| **6** | `0x40` | **G7** | Rangée supérieure, touche 7 (droite) |
| **7** | `0x80` | **G8** | Rangée 2, touche 1 (gauche) |

#### Octet 4 : Touches G9 à G16 (Bitmask, actif à `1`)
| Bit | Masque | Touche Associée | Emplacement Physique |
| :---: | :---: | :---: | :--- |
| **0** | `0x01` | **G9** | Rangée 2, touche 2 |
| **1** | `0x02` | **G10** | Rangée 2, touche 3 |
| **2** | `0x04` | **G11** | Rangée 2, touche 4 |
| **3** | `0x08` | **G12** | Rangée 2, touche 5 |
| **4** | `0x10` | **G13** | Rangée 2, touche 6 |
| **5** | `0x20` | **G14** | Rangée 2, touche 7 (droite) |
| **6** | `0x40` | **G15** | Rangée 3, touche 1 (gauche / Maj) |
| **7** | `0x80` | **G16** | Rangée 3, touche 2 |

#### Octet 5 : Touches G17 à G24 (Bitmask, actif à `1`)
| Bit | Masque | Touche Associée | Emplacement Physique |
| :---: | :---: | :---: | :--- |
| **0** | `0x01` | **G17** | Rangée 3, touche 3 |
| **1** | `0x02` | **G18** | Rangée 3, touche 4 |
| **2** | `0x04` | **G19** | Rangée 3, touche 5 (droite) |
| **3** | `0x08` | **G20** | Rangée 4 (inférieure), touche 1 (Ctrl) |
| **4** | `0x10` | **G21** | Rangée 4, touche 2 (Alt) |
| **5** | `0x20` | **G22** | Rangée 4, touche 3 (Espace principale) |
| **6** | `0x40` | *—* | Inutilisé (jamais vu à `1`) |
| **7** | `0x80` | *état* | **Pas une touche** : indicateur d'état du rétroéclairage, à `1` au repos |

> Les tables des octets 5 (bits 6–7), 6 et 7 ont été corrigées le 2026-10-15 d'après des captures
> sur un G13 réel (Windows, `GetInputReport`). La version précédente plaçait G23, G24, G25, BD, L1–L4,
> M1–M3 et MR un ou deux bits trop loin. La disposition mesurée est celle du pilote Linux `ecraven/g13`.

#### Octet 6 : Touches Utilitaires et Profils (Bitmask, actif à `1`)
| Bit | Masque | Touche Associée | Description |
| :---: | :---: | :---: | :--- |
| **0** | `0x01` | **BD** | Touche ronde rétroéclairage (à droite des boutons LCD) |
| **1** | `0x02` | **L1** | Bouton LCD 1 (bouton rond sous l'écran, le plus à gauche) |
| **2** | `0x04` | **L2** | Bouton LCD 2 (deuxième bouton sous l'écran) |
| **3** | `0x08` | **L3** | Bouton LCD 3 (troisième bouton sous l'écran) |
| **4** | `0x10` | **L4** | Bouton LCD 4 (quatrième bouton sous l'écran, le plus à droite) |
| **5** | `0x20` | **M1** | Touche de profil M1 (au-dessus des touches G) |
| **6** | `0x40` | **M2** | Touche de profil M2 |
| **7** | `0x80` | **M3** | Touche de profil M3 |

#### Octet 7 : MR, Touches Pouce, Clic Stick et Indicateurs (Bitmask, actif à `1`)
| Bit | Masque | Touche Associée | Description |
| :---: | :---: | :---: | :--- |
| **0** | `0x01` | **MR** | Touche d'enregistrement macro (Macro Record) |
| **1** | `0x02` | **G23** | Touche pouce latérale (gauche du stick) |
| **2** | `0x04` | **G24** | Touche pouce inférieure (sous le stick) |
| **3** | `0x08` | **G25** | Clic physique du mini-stick analogique |
| **4** | `0x10` | *—* | Inutilisé |
| **5..6**| `0x60` | *état* | **Pas des touches** : indicateurs de rétroéclairage (changent quand celui-ci bascule) |
| **7** | `0x80` | *état* | **Pas une touche** : bascule d'elle-même en permanence (bruit) |

Les bits d'état doivent être masqués avant toute détection de fronts, faute de quoi le pilote croit
une touche enfoncée en continu.

---

## 4. Protocole de Sortie : Contrôles et Affichage (Hôte -> Périphérique)

Toutes les commandes sortantes utilisent des **Feature Reports** (via `SetFeatureReport` / contrôle USB endpoint 0) ou des **Output Reports** (via `WriteFile` sur l'endpoint d'interruption `OUT`).

### 4.1 Rétroéclairage RGB : Feature Report `0x07` (5 octets)

Permet de configurer instantanément la couleur et l'intensité globale des LEDs du clavier et de l'écran.

```
+---------+---------+---------+---------+---------+
| Octet 0 | Octet 1 | Octet 2 | Octet 3 | Octet 4 |
+---------+---------+---------+---------+---------+
|   0x07  |  Rouge  |   Vert  |   Bleu  |   0x00  |
+---------+---------+---------+---------+---------+
```

* **Octet 0 :** `0x07` (Report ID obligatoire).
* **Octet 1 (Rouge) :** Intensité canal Rouge (`0x00` éteint à `0xFF` luminosité max).
* **Octet 2 (Vert) :** Intensité canal Vert (`0x00` à `0xFF`).
* **Octet 3 (Bleu) :** Intensité canal Bleu (`0x00` à `0xFF`).
* **Octet 4 :** `0x00` (Réservé / Mode statique).

> **Exemples :**
> * Rouge pur : `[0x07, 0xFF, 0x00, 0x00, 0x00]`
> * Blanc neutre 100% : `[0x07, 0xFF, 0xFF, 0xFF, 0x00]`
> * Éteint : `[0x07, 0x00, 0x00, 0x00, 0x00]`

### 4.2 LEDs d'Indication M-Keys : Feature Report `0x05` (5 octets)

Contrôle les 4 diodes électroluminescentes d'état situées sur les boutons M1, M2, M3 et MR.

```
+---------+---------+---------+---------+---------+
| Octet 0 | Octet 1 | Octet 2 | Octet 3 | Octet 4 |
+---------+---------+---------+---------+---------+
|   0x05  | LED Mask|   0x00  |   0x00  |   0x00  |
+---------+---------+---------+---------+---------+
```

* **Octet 0 :** `0x05` (Report ID obligatoire).
* **Octet 1 (Masque des LEDs) :**
  * Bit 0 (`0x01`) : Allume la LED **M1**
  * Bit 1 (`0x02`) : Allume la LED **M2**
  * Bit 2 (`0x04`) : Allume la LED **M3**
  * Bit 3 (`0x08`) : Allume la LED **MR**
  * Bits 4..7 : Non utilisés (`0`)
* **Octets 2, 3, 4 :** `0x00`.

> **Exemples :**
> * M1 uniquement actif : `[0x05, 0x01, 0x00, 0x00, 0x00]`
> * M3 + MR actifs : `[0x05, 0x0C, 0x00, 0x00, 0x00]`
> * Toutes éteintes : `[0x05, 0x00, 0x00, 0x00, 0x00]`

### 4.3 Affichage Écran LCD Monochrome (160×43) : Output Report `0x03` (992 octets)

L'écran LCD utilise un contrôleur graphique matriciel monochrome de **160 pixels de largeur par 43 pixels de hauteur**.  
Le rapport complet mesure exactement **992 octets** (`0x03E0`) :

$$\text{Taille totale} = 1\text{ (Report ID)} + 31\text{ (En-tête réservé)} + 960\text{ (Données graphiques)} = 992\text{ octets}$$

```
+----------+--------------------+------------------------------------+
|  Octet 0 |   Octets 1 à 31    |          Octets 32 à 991           |
+----------+--------------------+------------------------------------+
|   0x03   | En-tête (0x00...)  |  960 octets de Framebuffer Bitmap  |
+----------+--------------------+------------------------------------+
```

#### Organisation Mémoire du Framebuffer (Colonnes & Pages)
La zone de 960 octets est structurée en **6 bandes horizontales (pages)** de **160 octets (colonnes)** chacune :

$$6\text{ pages} \times 160\text{ colonnes} = 960\text{ octets}$$

Chaque octet encode une tranche verticale de **8 pixels** pour une colonne $X$ donnée ($0 \le X < 160$) :
* **Bit 0 (LSB) :** Pixel supérieur ($Y = \text{page} \times 8 + 0$)
* **Bit 7 (MSB) :** Pixel inférieur ($Y = \text{page} \times 8 + 7$)

Comme l'écran fait 43 pixels de haut ($43 = 5 \times 8 + 3$), les pages 0 à 4 sont entièrement visibles (lignes 0 à 39), et la page 5 utilise ses 3 premiers bits (lignes 40, 41, 42). Les bits 3 à 7 de la page 5 sont hors champ d'affichage.

#### Formule de Calcul d'Offset Pixel $(X, Y)$

Pour un pixel situé aux coordonnées cartésiennes $(X, Y)$ avec $0 \le X < 160$ et $0 \le Y < 43$ :

$$\text{Page} = \left\lfloor \frac{Y}{8} \right\rfloor$$

$$\text{Bit} = Y \pmod 8$$

$$\text{Offset Mémoire} = 32 + (\text{Page} \times 160) + X$$

* Pour allumer le pixel :
  $$\text{Buffer}[\text{Offset}] \mathrel{\vert}= (1 \ll \text{Bit})$$
* Pour éteindre le pixel :
  $$\text{Buffer}[\text{Offset}] \mathrel{\&}= \sim(1 \ll \text{Bit})$$

#### Taux de Rafraîchissement
* Fréquence recommandée : **30 Hz à 60 Hz** (intervalle d'envoi de trame entre 16 ms et 33 ms).
* L'écran maintient son image de manière statique tant qu'une nouvelle trame n'est pas transmise.

### 4.4 Mémoire Embarquée / Profils Nomades : Feature Report `0x06` (258 octets)

* **Taille :** 1 octet Report ID (`0x06`) + 1 octet de commande/index + 256 octets de charge utile = **258 octets**.
* Permet de lire et d'écrire la configuration matérielle par défaut (bindings de touches par défaut) conservée hors ligne sans pilote hôte.

---

## 5. Synthèse des Reports HID

| Report ID | Direction | Type USB | Longueur Totale | Rôle Technique |
| :---: | :---: | :---: | :---: | :--- |
| **`0x01`** | Device $\to$ PC | Interrupt IN | 8 octets | État des 29 boutons + Position analogique Stick X/Y |
| **`0x03`** | PC $\to$ Device | Output / Interrupt OUT | 992 octets | Framebuffer matriciel complet de l'écran LCD 160×43 |
| **`0x04`** | Bidirectionnel | Feature Report | 5 octets | Configuration et interrogation du statut du contrôleur |
| **`0x05`** | PC $\to$ Device | Feature Report | 5 octets | Pilotage des 4 LEDs indicatrices de profil (M1, M2, M3, MR) |
| **`0x06`** | Bidirectionnel | Feature Report | 258 octets | Accès lecture/écriture à la mémoire EEPROM de 256 octets |
| **`0x07`** | PC $\to$ Device | Feature Report | 5 octets | Pilotage du rétroéclairage RGB (intensité R, G, B) |
