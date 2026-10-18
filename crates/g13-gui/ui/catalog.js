// Catalogue of what a G-key can send, with labels in the chosen keyboard layout.
//
// Stored names are the daemon's KeyCode names = PHYSICAL positions with US
// labels (keycode.rs: KeyCode::name(), e.g. "W", "LeftCtrl", and "1" for
// Digit1): "W" is the key that prints Z on AZERTY. Labels shown
// here are what an AZERTY keyboard produces, so the user picks what they see.
"use strict";

const ICONS = {
  play: '<svg viewBox="0 0 24 24"><path d="M7 4.5v15l12-7.5z"/></svg>',
  prev: '<svg viewBox="0 0 24 24"><path d="M6 5v14M18 5 8 12l10 7z"/></svg>',
  next: '<svg viewBox="0 0 24 24"><path d="M18 5v14M6 5l10 7-10 7z"/></svg>',
  stop: '<svg viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" rx="1.5"/></svg>',
  mute: '<svg viewBox="0 0 24 24"><path d="M4 9.5h3.5L12 6v12l-4.5-3.5H4z"/><path d="m16 9.5 5 5M21 9.5l-5 5"/></svg>',
  voldown: '<svg viewBox="0 0 24 24"><path d="M4 9.5h3.5L12 6v12l-4.5-3.5H4z"/><path d="M15.5 9.5a3.5 3.5 0 0 1 0 5"/></svg>',
  volup: '<svg viewBox="0 0 24 24"><path d="M4 9.5h3.5L12 6v12l-4.5-3.5H4z"/><path d="M15.5 9.5a3.5 3.5 0 0 1 0 5M18 7a7 7 0 0 1 0 10"/></svg>',
};

// Keyboard drawn in the picker, one per layout: [KeyCode name, main label,
// secondary label (shift), width in units]. Codes are physical US positions, so
// the same code is the same key on every layout; only what is printed differs.
const R = (codes, labels) => codes.map((c, i) => [c, ...labels[i]]);
const LETTERS1 = ["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"];
const LETTERS2 = ["A", "S", "D", "F", "G", "H", "J", "K", "L"];
const LETTERS3 = ["Z", "X", "C", "V", "B", "N", "M"];
const own = (codes) => codes.map((c) => [c]);
const DIGITS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
// ISO layouts (AZERTY, QWERTZ, Italian, Spanish): Enter spans two rows, extra key left of Z
function iso({ row1, top, home, bottom, row2 = own(LETTERS1), row3 = own(LETTERS2), row4 = own(LETTERS3) }) {
  return [
    [["Grave", ...row1[0]], ...R(DIGITS, row1.slice(1, 11)), ["Minus", ...row1[11]], ["Equal", ...row1[12]], ["Backspace", "⌫", "", 1.6]],
    [["Tab", "⇥", "", 1.4], ...R(LETTERS1, row2), ["LeftBracket", ...top[0]], ["RightBracket", ...top[1]], ["Enter", "↵", "", 1.2]],
    [["CapsLock", "⇪", "", 1.7], ...R(LETTERS2, row3), ["Semicolon", ...home[0]], ["Apostrophe", ...home[1]], ["Backslash", ...home[2]]],
    [["LeftShift", "⇧", "", 1.3], ["IntlBackslash", "<", ">"], ...R(LETTERS3, row4), ["Comma", ...bottom[0]], ["Period", ...bottom[1]],
     ["Slash", ...bottom[2]], ["RightShift", "⇧", "", 2.2]],
  ];
}
const LAYOUTS = {
  azerty: iso({
    // number row: what AZERTY types without Shift is the big label, the digit is the small one
    row1: [["²"], ["&", "1"], ["é", "2"], ["\"", "3"], ["'", "4"], ["(", "5"], ["-", "6"], ["è", "7"], ["_", "8"], ["ç", "9"],
      ["à", "0"], [")", "°"], ["=", "+"]],
    row2: [["A"], ["Z"], ["E"], ["R"], ["T"], ["Y"], ["U"], ["I"], ["O"], ["P"]],
    top: [["^", "¨"], ["$", "£"]],
    row3: [["Q"], ["S"], ["D"], ["F"], ["G"], ["H"], ["J"], ["K"], ["L"]],
    home: [["M"], ["ù", "%"], ["*", "µ"]],
    row4: [["W"], ["X"], ["C"], ["V"], ["B"], ["N"], [",", "?"]],
    bottom: [[";", "."], [":", "/"], ["!", "§"]],
  }),
  qwertz: iso({
    row1: [["^", "°"], ["1", "!"], ["2", "\""], ["3", "§"], ["4", "$"], ["5", "%"], ["6", "&"], ["7", "/"], ["8", "("], ["9", ")"],
      ["0", "="], ["ß", "?"], ["´", "`"]],
    row2: [["Q"], ["W"], ["E"], ["R"], ["T"], ["Z"], ["U"], ["I"], ["O"], ["P"]],
    top: [["Ü"], ["+", "*"]],
    home: [["Ö"], ["Ä"], ["#", "'"]],
    row4: [["Y"], ["X"], ["C"], ["V"], ["B"], ["N"], ["M"]],
    bottom: [[",", ";"], [".", ":"], ["-", "_"]],
  }),
  it: iso({
    row1: [["\\", "|"], ["1", "!"], ["2", "\""], ["3", "£"], ["4", "$"], ["5", "%"], ["6", "&"], ["7", "/"], ["8", "("], ["9", ")"],
      ["0", "="], ["'", "?"], ["ì", "^"]],
    top: [["è", "é"], ["+", "*"]],
    home: [["ò", "ç"], ["à", "°"], ["ù", "§"]],
    bottom: [[",", ";"], [".", ":"], ["-", "_"]],
  }),
  es: iso({
    row1: [["º", "ª"], ["1", "!"], ["2", "\""], ["3", "·"], ["4", "$"], ["5", "%"], ["6", "&"], ["7", "/"], ["8", "("], ["9", ")"],
      ["0", "="], ["'", "?"], ["¡", "¿"]],
    top: [["`", "^"], ["+", "*"]],
    home: [["Ñ"], ["´", "¨"], ["ç", "Ç"]],
    bottom: [[",", ";"], [".", ":"], ["-", "_"]],
  }),
  // ANSI: Backslash above Enter, no key left of Z
  us: [
    [["Grave", "`", "~"], ...R(DIGITS, [["1", "!"], ["2", "@"], ["3", "#"], ["4", "$"], ["5", "%"], ["6", "^"], ["7", "&"], ["8", "*"],
      ["9", "("], ["0", ")"]]), ["Minus", "-", "_"], ["Equal", "=", "+"], ["Backspace", "⌫", "", 1.6]],
    [["Tab", "⇥", "", 1.4], ...own(LETTERS1), ["LeftBracket", "[", "{"], ["RightBracket", "]", "}"], ["Backslash", "\\", "|", 1.2]],
    [["CapsLock", "⇪", "", 1.7], ...own(LETTERS2), ["Semicolon", ";", ":"], ["Apostrophe", "'", "\""], ["Enter", "↵", "", 1.9]],
    [["LeftShift", "⇧", "", 2.3], ...own(LETTERS3), ["Comma", ",", "<"], ["Period", ".", ">"], ["Slash", "/", "?"],
      ["RightShift", "⇧", "", 2.3]],
  ],
};
// keys without an explicit label print their own name (letters)
for (const rows of Object.values(LAYOUTS)) for (const row of rows) for (const k of row) if (k.length === 1) k.push(k[0]);

/** Layout used by default for each interface language. */
const LANG_LAYOUT = { fr: "azerty", en: "us", de: "qwertz", it: "it", es: "es" };

// Keys whose label is a word, per language (same order as WORD_CODES), then the keypad prefix.
const WORD_CODES = ["Escape", "Insert", "Delete", "Home", "End", "PageUp", "PageDown", "PrintScreen", "ScrollLock", "Pause",
  "NumLock", "Enter", "Space", "CapsLock", "LeftCtrl", "RightCtrl", "LeftShift", "RightShift", "LeftAlt", "RightAlt",
  "LeftMeta", "RightMeta", "Menu"];
const KEY_WORDS = {
  fr: ["Échap", "Inser", "Suppr", "Début", "Fin", "Pg↑", "Pg↓", "Impr", "Défil", "Pause", "VerrNum", "Entrée", "Espace",
    "Verr.Maj", "Ctrl", "Ctrl D", "Maj", "Maj D", "Alt", "AltGr", "Win", "Win D", "Menu", "Pavé"],
  en: ["Esc", "Ins", "Del", "Home", "End", "PgUp", "PgDn", "PrtSc", "ScrLk", "Pause", "NumLk", "Enter", "Space",
    "Caps", "Ctrl", "R Ctrl", "Shift", "R Shift", "Alt", "R Alt", "Win", "R Win", "Menu", "Num"],
  de: ["Esc", "Einfg", "Entf", "Pos1", "Ende", "Bild↑", "Bild↓", "Druck", "Rollen", "Pause", "Num", "Eingabe", "Leertaste",
    "Feststell", "Strg", "Strg R", "Umsch", "Umsch R", "Alt", "AltGr", "Win", "Win R", "Menü", "Num"],
  it: ["Esc", "Ins", "Canc", "Inizio", "Fine", "PagSu", "PagGiù", "Stamp", "BlocScorr", "Pausa", "BlocNum", "Invio", "Spazio",
    "BlocMaiusc", "Ctrl", "Ctrl D", "Maiusc", "Maiusc D", "Alt", "AltGr", "Win", "Win D", "Menu", "Num"],
  es: ["Esc", "Insert", "Supr", "Inicio", "Fin", "RePág", "AvPág", "ImprPant", "BloqDespl", "Pausa", "BloqNum", "Intro", "Espacio",
    "BloqMayús", "Ctrl", "Ctrl D", "Mayús", "Mayús D", "Alt", "AltGr", "Win", "Win D", "Menú", "Num"],
};

/** Rows of the keyboard tab (filled by applyLayout). */
const KEYBOARD_ROWS = [];

// "TOUCHES" tab: [KeyCode name, label]
const BASIC_GROUPS = [
  ["Fonctions", ["Escape", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"]],
  ["Fonctions étendues", ["F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24"]],
  ["Navigation", ["Insert", "Delete", "Home", "End", "PageUp", "PageDown", "Up", "Down", "Left", "Right",
                  "PrintScreen", "ScrollLock", "Pause"]],
  ["Édition", ["Enter", "Tab", "Backspace", "Space", "CapsLock", "NumLock"]],
  ["Modificateurs seuls", ["LeftCtrl", "RightCtrl", "LeftShift", "RightShift", "LeftAlt", "RightAlt",
                           "LeftMeta", "RightMeta", "Menu"]],
  ["Pavé numérique", ["Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7",
                      "Numpad8", "Numpad9", "NumpadAdd", "NumpadSubtract", "NumpadMultiply", "NumpadDivide",
                      "NumpadDecimal", "NumpadEnter"]],
];

const MEDIA = [
  ["MediaPlayPause", "Lecture / pause", ICONS.play], ["MediaPrevious", "Précédent", ICONS.prev],
  ["MediaNext", "Suivant", ICONS.next], ["MediaStop", "Stop", ICONS.stop], ["VolumeMute", "Muet", ICONS.mute],
  ["VolumeDown", "Volume −", ICONS.voldown], ["VolumeUp", "Volume +", ICONS.volup],
];

// Mouse tab: [action object, short label, title]
const MOUSE = [
  [{ mouse: "Left" }, "Clic G", "Clic gauche"], [{ mouse: "Right" }, "Clic D", "Clic droit"],
  [{ mouse: "Middle" }, "Clic M", "Clic milieu"], [{ mouse: "Back" }, "Btn 4", "Bouton précédent (X1)"],
  [{ mouse: "Forward" }, "Btn 5", "Bouton suivant (X2)"], [{ wheel: 1 }, "Mol ↑", "Molette vers le haut"],
  [{ wheel: -1 }, "Mol ↓", "Molette vers le bas"], [{ wheel: -1, horizontal: true }, "Mol ←", "Molette à gauche"],
  [{ wheel: 1, horizontal: true }, "Mol →", "Molette à droite"],
];

// Gamepad tab (virtual Xbox 360 controller): [caption, [[button id, short label, title]…]]
const PAD_GROUPS = [
  ["Boutons", [["A", "Ⓐ", "Bouton A"], ["B", "Ⓑ", "Bouton B"], ["X", "Ⓧ", "Bouton X"], ["Y", "Ⓨ", "Bouton Y"]]],
  ["Gâchettes", [["LB", "LB", "Bumper gauche"], ["RB", "RB", "Bumper droit"],
    ["LT", "LT", "Gâchette gauche (appui complet)"], ["RT", "RT", "Gâchette droite (appui complet)"]]],
  ["Croix directionnelle", [["Up", "✚↑", "Croix haut"], ["Down", "✚↓", "Croix bas"],
    ["Left", "✚←", "Croix gauche"], ["Right", "✚→", "Croix droite"]]],
  ["Centre et sticks", [["Back", "Back", "Back (Select / Affichage)"], ["Start", "Start", "Start (Menu)"],
    ["LS", "L3", "Clic du stick gauche"], ["RS", "R3", "Clic du stick droit"]]],
];
const PAD = PAD_GROUPS.flatMap(([, list]) => list);

// Built-in functions a mouse button can hold (G502 X PLUS memory): [id, short label, title]
const MOUSE_SPECIAL_GROUPS = [
  ["Sensibilité", [["dpi_up", "DPI +", "Palier DPI suivant"], ["dpi_down", "DPI −", "Palier DPI précédent"],
    ["dpi_cycle", "DPI ⟳", "Paliers DPI en boucle"], ["dpi_default", "DPI défaut", "Revenir au palier DPI par défaut"],
    ["dpi_shift", "Sniper", "Palier « sniper » tant que le bouton est maintenu"]]],
  ["Fonctions de la souris", [["gshift", "G-shift", "Seconde fonction des autres boutons tant que celui-ci est maintenu"],
    ["profile_next", "Profil +", "Profil interne suivant"], ["profile_previous", "Profil −", "Profil interne précédent"],
    ["profile_cycle", "Profil ⟳", "Profils internes en boucle"], ["battery", "Batterie", "Affiche le niveau de batterie sur la souris"],
    ["wheel_left", "Mol ←", "Molette inclinée à gauche"], ["wheel_right", "Mol →", "Molette inclinée à droite"]]],
];
const MOUSE_SPECIALS = MOUSE_SPECIAL_GROUPS.flatMap(([, list]) => list);

// Labels for every KeyCode name in the chosen layout and language (filled by applyLayout).
const KEY_LABELS = {};
const MOD_SHORT = {};

/** Switch the picker keyboard and every key label to `layout` (azerty, us, qwertz, it, es). */
function applyLayout(layout, lang) {
  const words = KEY_WORDS[lang] || KEY_WORDS.fr;
  const w = Object.fromEntries(WORD_CODES.map((code, i) => [code, words[i]]));
  const pad = words[WORD_CODES.length];
  KEYBOARD_ROWS.length = 0;
  for (const row of LAYOUTS[layout] || LAYOUTS.azerty) KEYBOARD_ROWS.push(row.map((k) => [...k]));
  KEYBOARD_ROWS.push([["LeftCtrl", w.LeftCtrl, "", 1.3], ["LeftMeta", "Win"], ["LeftAlt", "Alt"], ["Space", w.Space, "", 5.2],
    ["RightAlt", w.RightAlt], ["Menu", "☰"], ["RightCtrl", w.LeftCtrl, "", 1.3], ["Left", "←"], ["Down", "↓"],
    ["Up", "↑"], ["Right", "→"]]);
  for (const k of Object.keys(KEY_LABELS)) delete KEY_LABELS[k];
  for (const row of KEYBOARD_ROWS) for (const [code, main] of row) KEY_LABELS[code] = main;
  Object.assign(KEY_LABELS, w, {
    Tab: "Tab", Backspace: "⌫", NumpadAdd: pad + " +", NumpadSubtract: pad + " −", NumpadMultiply: pad + " ×",
    NumpadDivide: pad + " ÷", NumpadDecimal: pad + " .", NumpadEnter: pad + " ↵",
    MediaPlayPause: "⏯", MediaPrevious: "⏮", MediaNext: "⏭", MediaStop: "⏹",
    VolumeMute: t("Muet"), VolumeDown: "Vol −", VolumeUp: "Vol +",
  });
  for (let i = 0; i <= 9; i++) KEY_LABELS["Numpad" + i] = pad + " " + i;
  for (let i = 1; i <= 24; i++) KEY_LABELS["F" + i] = "F" + i;
  Object.assign(MOD_SHORT, { LeftCtrl: w.LeftCtrl, RightCtrl: w.LeftCtrl, LeftShift: w.LeftShift, RightShift: w.LeftShift,
    LeftAlt: "Alt", RightAlt: w.RightAlt, LeftMeta: "Win", RightMeta: "Win" });
}
applyLayout("azerty", "fr");

// Aliases accepted by the daemon's KeyCode::parse (hand-written files).
const KEY_ALIASES = {
  esc: "Escape", ctrl: "LeftCtrl", control: "LeftCtrl", lctrl: "LeftCtrl", rctrl: "RightCtrl",
  shift: "LeftShift", lshift: "LeftShift", rshift: "RightShift", alt: "LeftAlt", lalt: "LeftAlt",
  altgr: "RightAlt", ralt: "RightAlt", win: "LeftMeta", super: "LeftMeta", meta: "LeftMeta", cmd: "LeftMeta",
  return: "Enter", del: "Delete", ins: "Insert", pgup: "PageUp", pgdn: "PageDown", pgdown: "PageDown",
  playpause: "MediaPlayPause",
};
const ALL_CODES = Object.keys(KEY_LABELS);

function canonicalKey(name) {
  const low = name.trim().toLowerCase();
  if (KEY_ALIASES[low]) return KEY_ALIASES[low];
  return ALL_CODES.find((c) => c.toLowerCase() === low) || name.trim();
}

const MODIFIERS = ["LeftCtrl", "RightCtrl", "LeftShift", "RightShift", "LeftAlt", "RightAlt", "LeftMeta", "RightMeta"];

/** Short human label of a stored action (string chord or object). */
function actionLabel(a) {
  if (a == null) return "";
  if (isDance(a)) return (a.tap != null ? actionLabel(a.tap) : "·") + "⁺";
  if (typeof a === "string") {
    const keys = a.split("+").map(canonicalKey);
    if (keys.length === 1) return KEY_LABELS[keys[0]] || keys[0];
    return keys.map((k) => (MODIFIERS.includes(k) ? MOD_SHORT[k] : KEY_LABELS[k] || k)).join("+");
  }
  if (a.mouse) {
    const hit = MOUSE.find(([o]) => o.mouse && o.mouse.toLowerCase() === a.mouse.toLowerCase());
    return hit ? t(hit[1]) : a.mouse;
  }
  if (a.wheel !== undefined) {
    const arrow = a.horizontal ? (a.wheel > 0 ? "→" : "←") : a.wheel > 0 ? "↑" : "↓";
    return t("Mol") + " " + arrow + (Math.abs(a.wheel) > 1 ? "×" + Math.abs(a.wheel) : "");
  }
  if (a.macro) return "▶ " + a.macro;
  if (a.special) {
    const hit = MOUSE_SPECIALS.find(([id]) => id === a.special);
    return hit ? (hit[1].startsWith("Mol ") ? t("Mol") + hit[1].slice(3) : t(hit[1])) : a.special;
  }
  if (a.raw) return t("Macro");
  if (a.pad) {
    const low = a.pad.toLowerCase();
    const alias = { l3: "LS", r3: "RS", select: "Back", view: "Back", menu: "Start" }[low];
    const hit = PAD.find(([id]) => id.toLowerCase() === low.replace(/^dpad/, "") || id === alias);
    return hit ? hit[1] : a.pad;
  }
  return "?";
}

/** Modes of a multi-state G13 key: [field, name, tooltip]. */
const DANCE_SLOTS = [
  ["tap", "Tap", "Appui court"],
  ["hold", "Hold", "Appui maintenu"],
  ["double_tap", "Tap-Tap", "Deux appuis courts"],
  ["tap_hold", "Tap-Hold", "Appui court puis maintenu"],
];
const isDance = (a) => a != null && typeof a === "object" && DANCE_SLOTS.some(([k]) => k in a);
/** Modes → stored action: a plain action when only Tap is set, null when nothing is. */
function danceValue(d) {
  const out = {};
  for (const [k] of DANCE_SLOTS) if (d[k] != null) out[k] = d[k];
  const keys = Object.keys(out);
  if (!keys.length) return null;
  return keys.length === 1 && keys[0] === "tap" ? out.tap : out;
}
/** "Tap D · Hold Ctrl" */
function danceSummary(a) {
  return DANCE_SLOTS.filter(([k]) => a[k] != null).map(([k, name]) => name + " " + actionLabel(a[k])).join(" · ");
}

/** Same action? (for highlighting the current one in the picker) */
function sameAction(a, b) {
  if (a == null || b == null) return false;
  if (typeof a === "string" && typeof b === "string") {
    const n = (s) => s.split("+").map(canonicalKey).join("+");
    return n(a) === n(b);
  }
  return JSON.stringify(a) === JSON.stringify(b);
}

const MACRO_MODES = [
  ["once", "Une fois par appui"],
  ["repeat_while_held", "Répéter tant que la touche est maintenue"],
  ["toggle", "Marche / arrêt à chaque appui"],
];

// Macro step editor: [op, label, argument kind]
const STEP_OPS = [
  ["down", "Appuyer touche", "key"], ["up", "Relâcher touche", "key"],
  ["mouse_down", "Appuyer bouton souris", "button"], ["mouse_up", "Relâcher bouton souris", "button"],
  ["wheel", "Molette (crans)", "number"], ["wait", "Pause (ms)", "number"],
  // "wait_random 500 30": 500 ms plus a random 0 to 30 ms, drawn again at each pass
  ["wait_random", "Pause aléatoire (ms)", "random"],
];
