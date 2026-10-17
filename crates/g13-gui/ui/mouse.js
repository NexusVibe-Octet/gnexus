// Mouse page (G502 X PLUS…), laid out like the G13's: the blueprint in the
// middle (top view / left profile), the layer and profile pills on the left,
// "Paramètres avancés" under the drawing (DPI bar, lighting, report rate,
// onboard profiles, macros), Annuler / Enregistrer at the bottom.
//
// Onboard profiles on: the mouse follows the onboard profile chosen in the
// advanced settings; the page edits the profile picked there (the active one by
// default). Lighting and DPI of the active profile are tried live (RAM),
// "Enregistrer dans la souris" writes the edited profile into the mouse.
//
// Onboard profiles off (real time, status.host): the PROFILS pills are G13
// Studio's own profiles (one per application, the 1st everywhere else); the
// page edits the pill chosen, "Enregistrer" saves it in G Nexus.
"use strict";

const BATTERY_ICON = (pct, charging) => `<svg class="batt-ic" viewBox="0 0 26 14"><rect x="1" y="1.5" width="21" height="11" rx="2.5"/>
  <path d="M24 5v4"/><rect class="lvl" x="3" y="3.5" width="${(17 * Math.max(0, Math.min(100, pct))) / 100}" height="7" rx="1"/>
  ${charging ? '<path class="bolt" d="M13 3.2 9.6 7.4h3l-1.3 3.4 3.6-4.4h-3z"/>' : ""}</svg>`;

// The mouse's usual buttons (slot order), for a new profile; the first three are
// what "Effacer les boutons" keeps so that the mouse stays usable.
const MOUSE_DEFAULT_BUTTONS = [{ mouse: "Left" }, { mouse: "Right" }, { mouse: "Middle" }, { mouse: "Back" },
  { special: "dpi_shift" }, { mouse: "Forward" }, { special: "wheel_left" }, { special: "wheel_right" },
  { special: "profile_cycle" }, { special: "dpi_up" }, { special: "dpi_down" }];
const MOUSE_KEEP = MOUSE_DEFAULT_BUTTONS.slice(0, 3);

const Mouse = {
  status: null, // last MouseStatus from the back-end
  draft: null, // settings being edited
  editSector: null, // onboard profile being edited (null = the active one)
  editSoft: null, // real time: G Nexus profile being edited (null = the one in use)
  pendingSoft: null, // index of a profile just added (edited once it exists)
  saving: false,
  previewTimer: null,
  focusDpi: null, // index of the DPI step being edited (tried live)
  layer: "normal", // buttons shown: "normal" or "gshift"
  advTab: "dpi",

  init() {
    MouseDevice.build($("#mdev"), (slot, id) => this.pickButton(slot, id));
    $("#ms-layer").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-layer]");
      if (b) { this.layer = b.dataset.layer; this.renderButtons(); }
    });
    // the overlay has the same tabs in its head
    const ov = $("#ov-devs");
    if (ov) {
      ov.innerHTML = $("#dev-tabs").innerHTML;
      ov.querySelectorAll("[id]").forEach((n) => n.removeAttribute("id"));
    }
    $$(".dev-tabs").forEach((n) => n.addEventListener("click", (e) => {
      const b = e.target.closest("button[data-dev]");
      if (b) showDevice(b.dataset.dev);
    }));
    $("#ms-advanced").addEventListener("click", () => this.openAdvanced());
    $("#ms-adv-tabs").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-tab]");
      if (b) { this.advTab = b.dataset.tab; this.renderAdvanced(); }
    });
    this.initDpiBar();
    $("#ms-rate").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-hz]");
      if (b && this.draft) { this.draft.report_rate_hz = +b.dataset.hz; this.renderAdvanced(); }
    });
    this.initLighting();
    // macros (real time): the G13's editor, on the draft
    this.macroEd = new MacroEditor({
      list: $("#ms-macro-list"),
      edit: $("#ms-macro-edit"),
      get: () => this.draftMacros(),
      set: (m) => { if (this.draft) this.draft.macros = m; },
      changed: () => { this.renderButtons(); this.renderFooter(); },
      renamed: (old, nn) => this.rebindMacro(old, nn),
      deleted: (name) => this.rebindMacro(name, null),
    });
    $("#ms-onboard-list").addEventListener("click", (e) => {
      const row = e.target.closest("[data-sector]");
      if (!row || !this.status) return;
      const sector = +row.dataset.sector;
      const act = e.target.closest("[data-act]")?.dataset.act;
      if (act === "use") invoke("select_mouse_profile", { sector }).catch((x) => toast(String(x), true));
      else if (act === "enable" || act === "disable") {
        invoke("enable_mouse_profile", { sector, enabled: act === "enable" }).catch((x) => toast(String(x), true));
      } else this.edit(sector);
    });
    $$(".ms-revert").forEach((b) => b.addEventListener("click", () => {
      this.draft = structuredClone(this.slotSettings() ?? null);
      this.focusDpi = null;
      invoke("revert_mouse").catch((e) => toast(String(e), true));
      this.render();
    }));
    $$(".ms-save").forEach((b) => b.addEventListener("click", async () => {
      if (!this.draft || !this.dirty()) return;
      this.saving = true;
      this.renderFooter();
      try {
        if (this.host()) await invoke("save_mouse_soft", { index: this.softIndex(), profile: { ...this.softProfile(), settings: this.draft } });
        else await invoke("apply_mouse", { settings: this.draft, sector: this.editSector });
      }
      catch (e) { this.saving = false; toast(String(e), true); this.renderFooter(); }
    }));
    $("#ms-onboard-on").addEventListener("change", (e) => {
      if (this.draft && this.dirty() && !confirm(t("Abandonner les modifications non enregistrées ?"))) { e.target.checked = !e.target.checked; return; }
      this.editSector = null;
      this.editSoft = null;
      invoke("set_mouse_host", { on: !e.target.checked }).catch((x) => toast(String(x), true));
    });
    $("#ms-soft").addEventListener("click", (e) => {
      const b = e.target.closest("button.bank");
      if (!b || b.disabled || !this.host()) return;
      if (b.dataset.add != null) this.newSoft(false);
      else this.editSoftProfile(+b.dataset.i);
    });
    $("#ms-apps-bound").addEventListener("click", (e) => {
      const b = e.target.closest("[data-exe]");
      if (b) this.setApps(this.softProfile().apps.filter((a) => a !== b.dataset.exe));
    });
    $("#ms-apps-detected").addEventListener("click", (e) => {
      const b = e.target.closest(".app-chip");
      if (b && !b.disabled) this.bindApp(b.dataset.exe);
    });
    $("#ms-apps-refresh").addEventListener("click", () => this.renderApps());
    $("#ms-apps-browse").addEventListener("click", async () => {
      try { const exe = await invoke("pick_exe"); if (exe) this.bindApp(exe); } catch (x) { toast(String(x), true); }
    });
    $("#ms-pin").addEventListener("click", () => {
      const st = this.status, i = this.softIndex();
      const pinned = st?.soft_pinned === i ? null : i;
      invoke("pin_mouse_soft", { index: pinned }).catch((x) => toast(String(x), true));
      toast(pinned != null ? t("Profil actif partout sur la souris jusqu'à l'arrêt du test") : t("Test arrêté : bascule automatique rétablie"));
    });
    $("#ms-default").addEventListener("click", () => {
      invoke("set_mouse_soft_default", { index: this.softIndex() }).catch((x) => toast(String(x), true));
      toast(t("Profil par défaut : il s'applique maintenant partout"));
    });
    $("#ms-assign").addEventListener("click", () => this.openAdvanced("apps"));
    $("#ms-new").addEventListener("click", () => this.newSoft(false));
    $("#ms-dup").addEventListener("click", () => this.newSoft(true));
    $("#ms-rename").addEventListener("click", async () => {
      const p = this.softProfile();
      if (!this.host() || !p) return;
      const name = await ask(t("RENOMMER LE PROFIL"), this.softName(this.softIndex()));
      if (!name) return;
      invoke("save_mouse_soft", { index: this.softIndex(), profile: { ...p, name } }).catch((x) => toast(String(x), true));
    });
    $("#ms-clear").addEventListener("click", async () => {
      if (!this.draft?.buttons) return;
      const ok = await ask(t("EFFACER LES BOUTONS DU PROFIL ?"), "", { confirmOnly: true, ok: t("EFFACER") });
      if (!ok) return;
      // the mouse must stay usable: left click, right click and wheel click are kept
      const keep = MOUSE_KEEP;
      this.draft.buttons = this.draft.buttons.map((_, i) => keep[i] ? structuredClone(keep[i]) : null);
      this.draft.gshift = this.draft.gshift.map(() => null);
      this.preview(true);
      this.render();
      toast(t("Boutons effacés (clic gauche, clic droit et molette gardés) : enregistrez pour appliquer"));
    });
    $("#ms-delete").addEventListener("click", async () => {
      const i = this.softIndex();
      if (!this.host() || !i) return;
      const ok = await ask(t("SUPPRIMER « {name} » ?", { name: this.softName(i) }), "", { confirmOnly: true, ok: t("SUPPRIMER") });
      if (!ok) return;
      this.editSoft = null;
      invoke("delete_mouse_soft", { index: i }).catch((x) => toast(String(x), true));
    });
    invoke("get_mouse").then((st) => { if (st) this.update(st); }).catch(() => {});
    this.renderTab();
  },

  show(on) {
    showDevice(on ? "mouse" : "g13");
  },

  openAdvanced(tab) {
    if (tab) this.advTab = tab;
    this.renderAdvanced();
    openSheet("#ms-adv");
  },

  // ---- data -------------------------------------------------------------------------

  /** Real time: onboard profiles off, G Nexus's profiles in use. */
  host() {
    return !!this.status?.host;
  },
  /** Real time: index of the G Nexus profile being edited. */
  softIndex() {
    const n = this.status?.soft_profiles?.length || 0;
    const i = this.editSoft ?? this.status?.soft_active ?? 0;
    return Math.min(i, Math.max(0, n - 1));
  },
  softProfile() {
    return this.status?.soft_profiles?.[this.softIndex()] ?? null;
  },
  /** The profile being edited (onboard: its number). */
  sector() {
    return this.editSector ?? this.status?.profile ?? null;
  },
  /** "profil interne 2" / "profil 2" (real time), for the sub-titles. */
  profileName() {
    return this.host() ? t("profil {n}", { n: this.softIndex() + 1 }) : t("profil interne {n}", { n: this.sector() });
  },
  /** Saved settings of the profile being edited. */
  slotSettings() {
    const st = this.status;
    if (!st) return null;
    if (this.host()) return this.softProfile()?.settings ?? null;
    if (this.editSector == null || this.editSector === st.profile) return st.settings;
    return st.profiles?.find((p) => p.sector === this.editSector)?.settings ?? null;
  },
  /** Live changes only make sense on the profile the mouse is using. */
  editingActive() {
    if (this.host()) return this.softIndex() === this.status?.soft_active;
    return this.editSector == null || this.editSector === this.status?.profile;
  },

  editSoftProfile(i) {
    if (i === this.softIndex()) return;
    if (this.draft && this.dirty() && !confirm(t("Abandonner les modifications non enregistrées ?"))) return;
    invoke("revert_mouse").catch(() => {});
    this.editSoft = i;
    this.draft = structuredClone(this.slotSettings() ?? null);
    this.focusDpi = null;
    this.render();
  },

  /** Name shown for G Nexus profile `i`. */
  softName(i) {
    return this.status?.soft_profiles?.[i]?.name || t("Profil {n}", { n: i + 1 });
  },
  /** Index of the default profile (used where no application has its own). */
  softDefault() {
    return this.status?.soft_default ?? 0;
  },

  /** Real time: what the profile shown does, and TESTER PARTOUT / UTILISER PAR DÉFAUT (as for the G13). */
  renderScope() {
    const st = this.status, host = this.host() && st?.connected;
    for (const id of ["#ms-pin", "#ms-default", "#ms-assign"]) $(id).classList.toggle("hidden", !host);
    $(".ms-bar").classList.remove("warn");
    if (!host) return false;
    const i = this.softIndex(), p = this.softProfile();
    const name = esc(this.softName(i)), def = this.softDefault();
    const apps = p?.apps || [];
    const pinned = st.soft_pinned;
    $("#ms-pin").textContent = pinned === i ? t("ARRÊTER LE TEST") : t("TESTER PARTOUT");
    $("#ms-pin").classList.toggle("primary", pinned === i);
    $("#ms-default").classList.toggle("hidden", i === def);
    $("#ms-assign").classList.toggle("hidden", apps.length > 0);
    let html, warn = false;
    if (pinned != null) {
      html = t("Test en cours : « <b>{name}</b> » est actif sur la souris <b>dans toutes les applications</b> (associations et profil par défaut ignorés) jusqu'à « Arrêter le test ».", { name: esc(this.softName(pinned)) });
      warn = pinned !== i;
    } else if (apps.length) {
      html = t("« <b>{name}</b> » s'active tout seul dans : <b>{apps}</b>.", { name, apps: esc(apps.join(", ")) }) +
        (i === def ? " " + t("Partout ailleurs aussi (profil par défaut ★).") : "");
    } else if (i === def) {
      html = t("« <b>{name}</b> » est le profil par défaut ★ : il s'applique partout où aucun profil de jeu n'est associé.", { name });
    } else {
      warn = true;
      html = t("« <b>{name}</b> » n'est associé à aucun jeu : c'est « <b>{def}</b> » ★ qui s'applique partout.", { name, def: esc(this.softName(def)) });
    }
    $(".ms-bar").classList.toggle("warn", warn);
    $("#ms-state").innerHTML = "· " + html + (st.charging ? " · " + esc(t("en charge")) : "");
    return true;
  },

  /** New G Nexus profile, for no application yet: the mouse's usual buttons, or a copy of the one shown. */
  async newSoft(copy) {
    const st = this.status;
    if (!this.host() || !st) return;
    if ((st.soft_profiles?.length || 0) >= 5) { toast(t("5 profils au plus : supprimez-en un d'abord"), true); return; }
    const base = this.slotSettings() ?? this.draft;
    if (!base) return;
    const n = st.soft_profiles.length + 1;
    const name = await ask(copy ? t("DUPLIQUER LE PROFIL") : t("NOUVEAU PROFIL"),
      copy ? t("{name} (copie)", { name: this.softName(this.softIndex()) }) : "", { placeholder: t("nom du jeu ou de l'application") });
    if (name == null || name === false) return;
    const settings = structuredClone(base);
    if (!copy) {
      settings.buttons = MOUSE_DEFAULT_BUTTONS.map((a) => structuredClone(a));
      settings.gshift = settings.gshift.map(() => null);
    }
    this.pendingSoft = n - 1;
    invoke("save_mouse_soft", { index: null, profile: { name: name || "", apps: [], settings } }).catch((x) => toast(String(x), true));
  },

  /** Save the applications of the profile shown (its saved settings, not the draft). */
  setApps(apps) {
    const p = this.softProfile();
    if (!p) return;
    invoke("save_mouse_soft", { index: this.softIndex(), profile: { ...p, apps } }).catch((x) => toast(String(x), true));
  },
  bindApp(exe) {
    const p = this.softProfile();
    if (!p) return;
    const lower = exe.toLowerCase();
    // an application has one profile: taken from another one if needed
    (this.status.soft_profiles || []).forEach((o, i) => {
      if (i !== this.softIndex() && o.apps.some((a) => a.toLowerCase() === lower)) {
        invoke("save_mouse_soft", { index: i, profile: { ...o, apps: o.apps.filter((a) => a.toLowerCase() !== lower) } }).catch(() => {});
      }
    });
    if (!p.apps.some((a) => a.toLowerCase() === lower)) this.setApps([...p.apps, exe]);
  },

  async renderApps() {
    const p = this.softProfile();
    if (!p) return;
    const i = this.softIndex();
    $("#ms-apps-help").textContent = i === this.softDefault()
      ? t("Le profil par défaut ★ sert dans toutes les applications qui n'ont pas leur propre profil. Associez-lui aussi des applications, ou créez un profil par jeu avec le + des pastilles PROFILS.")
      : t("Ce profil s'active quand l'une de ces applications est au premier plan.");
    $("#ms-apps-bound").innerHTML = p.apps.map((a) => `<button class="app-chip bound" data-exe="${esc(a)}" title="${esc(t("Retirer"))}"><b>${esc(a)}</b><em>${esc(t("retirer"))} ×</em></button>`).join("")
      || `<span class="muted" style="font-size:13px">${esc(i === this.softDefault() ? t("Toutes les autres applications.") : t("Aucune application associée."))}</span>`;
    let list;
    try { list = await invoke("detected_apps"); } catch { list = { running: [], recent: [] }; }
    const seen = new Set();
    const all = [...list.running, ...list.recent].filter((a) => !seen.has(a.exe.toLowerCase()) && seen.add(a.exe.toLowerCase()));
    const owner = (exe) => (this.status?.soft_profiles || []).findIndex((o) => o.apps.some((a) => a.toLowerCase() === exe.toLowerCase()));
    $("#ms-apps-detected").innerHTML = all.map((a) => {
      const o = owner(a.exe);
      return `<button class="app-chip ${o === i ? "bound" : ""}" data-exe="${esc(a.exe)}" ${o === i ? "disabled" : ""} title="${esc(a.title || "")}">
        <b>${esc(a.exe)}</b><span>${esc(a.title || "")}</span>${o >= 0 && o !== i ? `<em>${esc(t("actuellement : {name}", { name: t("profil {n}", { n: o + 1 }) }))}</em>` : ""}</button>`;
    }).join("") || `<span class="muted" style="font-size:13px">${esc(t("Aucune application ouverte détectée."))}</span>`;
  },

  edit(sector) {
    if (this.host()) return;
    if (this.draft && this.dirty() && !confirm(t("Abandonner les modifications non enregistrées ?"))) return;
    invoke("revert_mouse").catch(() => {});
    this.editSector = sector === this.status?.profile ? null : sector;
    this.draft = structuredClone(this.slotSettings() ?? null);
    this.focusDpi = null;
    this.render();
    this.renderAdvanced();
  },

  /** New status from the mouse thread. */
  update(st) {
    // in real time every click (on this page too) comes back as "pressed": only light the drawing,
    // re-rendering would replace the element under the pointer and lose the click
    const strip = (s) => JSON.stringify({ ...s, pressed: null });
    if (this.status && strip(st) === strip(this.status)) {
      this.status = st;
      MouseDevice.setPressed(st.pressed || []);
      return;
    }
    const hadEdits = this.draft && this.dirty();
    const wasSaving = this.saving;
    const wasHost = this.host();
    this.status = st;
    this.saving = false;
    if (this.editSector != null && !st.profiles?.some((p) => p.sector === this.editSector)) this.editSector = null;
    if (this.pendingSoft != null && st.soft_profiles?.length > this.pendingSoft) { this.editSoft = this.pendingSoft; this.pendingSoft = null; }
    if (this.editSoft != null && this.editSoft >= (st.soft_profiles?.length || 0)) this.editSoft = null;
    const switched = wasHost !== this.host();
    if (!hadEdits || wasSaving || switched || !this.slotSettings()) { this.draft = structuredClone(this.slotSettings() ?? null); this.focusDpi = null; }
    if (st.message && (wasSaving || st.error || switched)) {
      const ok = this.host() ? t("Profil {n} enregistré", { n: this.softIndex() + 1 }) : t("Enregistré dans la souris (profil interne {n})", { n: this.sector() });
      toast(st.error || switched ? t(st.message) : ok, st.error);
    }
    if (!this.advTabOk()) this.advTab = "onboard";
    this.renderTab();
    MouseDevice.setPressed(st.pressed || []);
    if (document.body.classList.contains("mouse-mode")) this.render();
    if ($("#ms-adv").classList.contains("open")) this.renderAdvanced();
  },

  dirty() {
    // no macros = an empty list (the back-end leaves it out)
    const norm = (s) => (s && s.macros && !Object.keys(s.macros).length ? (({ macros, ...rest }) => rest)(s) : s);
    return JSON.stringify(norm(this.draft)) !== JSON.stringify(norm(this.slotSettings() ?? null));
  },

  /** The draft's macros (created on first use). */
  draftMacros() {
    if (!this.draft) return null;
    if (!this.draft.macros) this.draft.macros = {};
    return this.draft.macros;
  },

  /** Rename / drop a macro in every button that uses it. */
  rebindMacro(old, nn) {
    for (const list of [this.draft?.buttons, this.draft?.gshift]) {
      list?.forEach((a, i) => { if (a && a.macro === old) list[i] = nn ? { macro: nn } : null; });
    }
    this.renderButtons();
  },

  snap(v) {
    const st = this.status;
    if (!st) return v;
    const step = st.dpi_step || 1;
    v = Math.round((v - st.dpi_min) / step) * step + st.dpi_min;
    return Math.max(st.dpi_min, Math.min(st.dpi_max, v || st.dpi_min));
  },

  /** Send the draft's lighting (and the DPI step being edited) to the mouse, at most every 60 ms. */
  preview(now = false) {
    clearTimeout(this.previewTimer);
    if (!this.editingActive()) return;
    const send = () => {
      if (!this.draft || !this.status?.connected) return;
      const dpi = this.focusDpi != null ? this.draft.dpi[this.focusDpi] : null;
      invoke("preview_mouse", { lighting: this.draft.lighting, dpi }).catch(() => {});
    };
    if (now) send(); else this.previewTimer = setTimeout(send, 60);
  },

  // ---- buttons ------------------------------------------------------------------------

  pickButton(slot, id) {
    const d = this.draft;
    if (!d?.buttons) return;
    const list = this.layer === "gshift" ? d.gshift : d.buttons;
    const name = id === "◂" ? t("Molette inclinée à gauche") : id === "▸" ? t("Molette inclinée à droite") : id;
    MouseDevice.setSelected(slot);
    openPickerFor({
      label: name,
      sub: (this.layer === "gshift" ? "G-SHIFT" : t("NORMAL")) + " · " + this.profileName(),
      mouse: true,
      macros: this.host() ? () => this.draftMacros() : null,
      newMacro: () => { this.draftMacros(); return this.macroEd.add(); },
      openMacros: () => setTimeout(() => this.openAdvanced("macros"), 0),
      get: () => list[slot] ?? null,
      set: (action) => {
        list[slot] = action ?? null;
        this.renderButtons();
        this.renderFooter();
      },
    });
  },

  renderButtons() {
    const d = this.draft;
    const gshift = this.layer === "gshift";
    $$("#ms-layer button").forEach((b) => b.classList.toggle("on", b.dataset.layer === this.layer));
    $("#ms-layer-help").classList.toggle("hidden", gshift);
    $("#ms-gshift-help").classList.toggle("hidden", !gshift);
    if (!d?.buttons) return;
    const list = gshift ? d.gshift : d.buttons;
    const isG = (a) => a && a.special === "gshift";
    $("#ms-gshift-none").classList.toggle("hidden", !gshift || d.buttons.some(isG));
    const labels = {}, titles = {}, disabled = new Set();
    list.forEach((a, i) => { labels[i] = a ? actionLabel(a) : ""; titles[i] = labels[i]; });
    // in the G-shift layer, the G-shift button itself only does G-shift
    if (gshift) d.buttons.forEach((a, i) => { if (isG(a)) { disabled.add(i); labels[i] = "G-shift"; titles[i] = "G-shift"; } });
    MouseDevice.setLabels(labels, titles, disabled);
    // the drawn light strip shows the chosen colours
    const l = d.lighting, el = $("#mdev");
    const [a, b] = l.effect === "gradient" ? [l.from, l.to] : l.color ? [l.color, l.color]
      : l.effect === "off" ? ["#3a3f4a", "#3a3f4a"] : l.effect === "cycle" ? ["#ff3355", "#3366ff"] : ["#ff4fb3", "#3d7bff"];
    el.style.setProperty("--strip-a", a);
    el.style.setProperty("--strip-b", b);
  },

  /** G Nexus's profiles (real time, one per game): locked while the onboard profiles are used. */
  renderSoftProfiles() {
    const st = this.status;
    const box = $("#ms-soft");
    if (!this.host()) {
      this.softHtml = null;
      box.innerHTML = `<button class="bank prof locked" disabled>1</button><button class="bank add locked" disabled>+</button>`;
      $("#ms-soft-label").textContent = t("VERROUILLÉS");
      box.title = t("Les profils G Nexus (un par jeu, en temps réel, le 1 partout par défaut) s'utilisent quand les profils intégrés de la souris sont désactivés (Paramètres avancés › Profils intégrés).");
      return;
    }
    box.title = "";
    const list = st.soft_profiles || [];
    const cur = this.softIndex();
    const html = list.map((p, i) => {
      const apps = p.apps.length ? p.apps.join(", ") : i === this.softDefault() ? t("toutes les autres applications") : t("aucune application");
      return `<button class="bank prof ${i === cur ? "on" : ""}" data-i="${i}" title="${esc(this.softName(i) + " · " + apps)}">${i + 1}${i === st.soft_active ? '<i class="act"></i>' : ""}</button>`;
    }).join("") + (list.length < 5 ? `<button class="bank add" data-add title="${esc(t("Nouveau profil (copie de celui affiché)"))}">+</button>` : "");
    if (html !== this.softHtml) { box.innerHTML = html; this.softHtml = html; }
    $("#ms-soft-label").textContent = t("PROFIL {n}", { n: cur + 1 });
  },

  // ---- DPI bar ----------------------------------------------------------------------------
  // Logarithmic scale: 100 → 25 600 DPI would squeeze the usual 400–3 200 into a corner.

  dpiPos(v) {
    const st = this.status;
    const lo = Math.log(st.dpi_min), hi = Math.log(st.dpi_max);
    return ((Math.log(v) - lo) / (hi - lo)) * 100;
  },
  dpiAt(pct) {
    const st = this.status;
    const lo = Math.log(st.dpi_min), hi = Math.log(st.dpi_max);
    return this.snap(Math.exp(lo + (hi - lo) * Math.max(0, Math.min(1, pct / 100))));
  },

  /** Keep the steps in increasing order (DPI + goes up), the default / sniper marks following them. */
  sortDpi(focus) {
    const d = this.draft;
    const rows = d.dpi.map((v, i) => ({ v, def: i === d.default_slot, snp: i === d.shift_slot, focus: i === focus }));
    rows.sort((a, b) => a.v - b.v);
    d.dpi = rows.map((r) => r.v);
    d.default_slot = Math.max(0, rows.findIndex((r) => r.def));
    d.shift_slot = Math.max(0, rows.findIndex((r) => r.snp));
    return rows.findIndex((r) => r.focus);
  },

  initDpiBar() {
    const bar = $("#dpi-bar");
    let drag = null;
    const pctOf = (e) => {
      const r = bar.querySelector(".dpi-track").getBoundingClientRect();
      return ((e.clientX - r.left) / r.width) * 100;
    };
    bar.addEventListener("pointerdown", (e) => {
      if (!this.draft || !this.status) return;
      const mark = e.target.closest(".dpi-mark");
      if (mark) {
        drag = +mark.dataset.i;
      } else {
        if (this.draft.dpi.length >= 5) { toast(t("5 paliers au plus : supprimez-en un d'abord")); return; }
        this.draft.dpi.push(this.dpiAt(pctOf(e)));
        drag = this.sortDpi(this.draft.dpi.length - 1);
      }
      this.focusDpi = drag;
      bar.setPointerCapture(e.pointerId);
      this.preview();
      this.renderDpi();
      this.renderFooter();
    });
    bar.addEventListener("pointermove", (e) => {
      if (drag == null || !(e.buttons & 1)) return;
      this.draft.dpi[drag] = this.dpiAt(pctOf(e));
      drag = this.sortDpi(drag);
      this.focusDpi = drag;
      this.preview();
      this.renderDpi();
      this.renderFooter();
    });
    bar.addEventListener("pointerup", () => { drag = null; });
    $("#dpi-edit").addEventListener("change", (e) => {
      if (!e.target.matches("input[type=number]") || this.focusDpi == null) return;
      this.draft.dpi[this.focusDpi] = this.snap(+e.target.value);
      this.focusDpi = this.sortDpi(this.focusDpi);
      this.preview(true);
      this.renderDpi();
      this.renderFooter();
    });
    $("#dpi-edit").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-act]");
      const d = this.draft, i = this.focusDpi;
      if (!b || !d || i == null) return;
      if (b.dataset.act === "default") d.default_slot = i;
      else if (b.dataset.act === "sniper") d.shift_slot = i;
      else if (b.dataset.act === "del" && d.dpi.length > 1) {
        d.dpi.splice(i, 1);
        const fix = (s) => (s === i ? 0 : s > i ? s - 1 : s);
        d.default_slot = Math.min(fix(d.default_slot), d.dpi.length - 1);
        d.shift_slot = Math.min(fix(d.shift_slot), d.dpi.length - 1);
        this.focusDpi = null;
      }
      this.renderDpi();
      this.renderFooter();
    });
  },

  renderDpi() {
    const st = this.status, d = this.draft;
    if (!st || !d) return;
    const ticks = [st.dpi_min, 400, 800, 1600, 3200, 6400, 12800, st.dpi_max]
      .filter((v, i, a) => v >= st.dpi_min && v <= st.dpi_max && a.indexOf(v) === i);
    $("#dpi-ticks").innerHTML = ticks.map((v) => `<span style="left:${this.dpiPos(v)}%">${v}</span>`).join("");
    const bar = $("#dpi-bar");
    bar.querySelectorAll(".dpi-mark").forEach((m) => m.remove());
    d.dpi.forEach((v, i) => {
      const m = document.createElement("button");
      m.className = "dpi-mark" + (i === d.default_slot ? " def" : "") + (i === d.shift_slot ? " snp" : "") + (i === this.focusDpi ? " sel" : "");
      m.dataset.i = i;
      m.style.left = this.dpiPos(v) + "%";
      m.innerHTML = `<b>${v}</b><i></i>`;
      m.title = t("Palier {n} : {d} DPI", { n: i + 1, d: v });
      bar.appendChild(m);
    });
    const i = this.focusDpi;
    $("#dpi-edit").innerHTML = i == null
      ? `<span class="muted small-print">${esc(t("Cliquez sur un marqueur pour le régler."))}</span>
         <span class="dpi-legend"><i class="def"></i>${esc(t("PAR DÉFAUT"))}<i class="snp"></i>${esc(t("SNIPER"))}</span>`
      : `<b>${esc(t("PALIER {n}", { n: i + 1 }))}</b>
         <input type="number" min="${st.dpi_min}" max="${st.dpi_max}" step="${st.dpi_step}" value="${d.dpi[i]}"><span class="unit">DPI</span>
         <button class="mini ${i === d.default_slot ? "on" : ""}" data-act="default" title="${esc(t("Palier utilisé à l'allumage de la souris"))}">${esc(t("PAR DÉFAUT"))}</button>
         <button class="mini snp ${i === d.shift_slot ? "on" : ""}" data-act="sniper" title="${esc(t("Palier utilisé tant que le bouton « sniper » est maintenu"))}">${esc(t("SNIPER"))}</button>
         <button class="mini" data-act="del" ${d.dpi.length < 2 ? "disabled" : ""}>${esc(t("SUPPRIMER"))}</button>
         <span class="grow"></span>
         <span class="muted small-print">${esc(t("de {min} à {max} DPI, par pas de {step}", { min: st.dpi_min, max: st.dpi_max, step: st.dpi_step }))}</span>`;
  },

  // ---- lighting ---------------------------------------------------------------------------

  initLighting() {
    $("#ms-effects").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-fx]");
      if (!b || !this.draft) return;
      const old = this.draft.lighting;
      const color = old.color || "#19E3CF";
      const period_ms = old.period_ms || 4000;
      // same key order as the back-end (serde), so that "unchanged" compares equal
      this.draft.lighting = { signature: { effect: "signature" }, off: { effect: "off" }, fixed: { effect: "fixed", color },
        breathing: { effect: "breathing", color, period_ms }, cycle: { effect: "cycle", period_ms },
        gradient: { effect: "gradient", from: old.from || "#FF4FB3", to: old.to || "#3D7BFF",
          period_ms: old.effect === "gradient" ? old.period_ms : 8000, reverse: !!old.reverse } }[b.dataset.fx];
      this.preview(true);
      this.renderLighting();
      this.renderButtons();
      this.renderFooter();
    });
    const colorPick = (get, setv) => () => {
      const l = this.draft?.lighting;
      const before = l && get(l);
      if (!before) return;
      openColor({
        initial: before,
        label: this.status?.name || t("SOURIS"),
        preview: (hex) => { setv(l, hex); this.preview(); this.renderLighting(); this.renderButtons(); this.renderFooter(); },
        restore: () => { setv(l, before); this.preview(true); setTimeout(() => this.openAdvanced("light"), 0); },
        done: () => setTimeout(() => this.openAdvanced("light"), 0),
      });
    };
    $("#ms-color").addEventListener("click", colorPick((l) => l.color, (l, v) => { l.color = v; }));
    $("#ms-grad-a").addEventListener("click", colorPick((l) => l.from, (l, v) => { l.from = v; }));
    $("#ms-grad-b").addEventListener("click", colorPick((l) => l.to, (l, v) => { l.to = v; }));
    $("#ms-grad-rev").addEventListener("click", () => {
      const l = this.draft?.lighting;
      if (!l || l.effect !== "gradient") return;
      l.reverse = !l.reverse;
      this.preview(true);
      this.renderLighting();
      this.renderFooter();
    });
    $("#ms-speed").addEventListener("input", (e) => {
      if (!this.draft?.lighting.period_ms) return;
      this.draft.lighting.period_ms = +e.target.value * 1000;
      this.renderLighting();
      this.preview();
      this.renderFooter();
    });
  },

  renderLighting() {
    const l = this.draft?.lighting;
    if (!l) return;
    $$("#ms-effects button").forEach((b) => b.classList.toggle("on", b.dataset.fx === l.effect));
    $("#ms-color-row").classList.toggle("hidden", !l.color);
    $("#ms-grad-row").classList.toggle("hidden", l.effect !== "gradient");
    $("#ms-grad-note").classList.toggle("hidden", l.effect !== "gradient");
    if (l.effect === "gradient") {
      $("#ms-grad-a").style.setProperty("--c", l.from);
      $("#ms-grad-b").style.setProperty("--c", l.to);
      $("#ms-grad-rev").classList.toggle("primary", !!l.reverse);
    }
    $("#ms-speed-row").classList.toggle("hidden", !l.period_ms);
    $("#ms-other").classList.toggle("hidden", l.effect !== "other");
    if (l.color) { $("#ms-color").style.setProperty("--c", l.color); $("#ms-color-val").textContent = l.color; }
    if (l.period_ms) {
      $("#ms-speed").value = Math.round(l.period_ms / 1000);
      $("#ms-speed-val").textContent = t("{s} s", { s: Math.round(l.period_ms / 1000) });
    }
    const preview = $("#ms-preview");
    preview.className = "ms-preview fx-" + l.effect + (l.reverse ? " rev" : "");
    preview.style.setProperty("--c", l.color || "#19E3CF");
    preview.style.setProperty("--a", l.from || "#FF4FB3");
    preview.style.setProperty("--b", l.to || "#3D7BFF");
    preview.style.setProperty("--p", (l.period_ms || 4000) + "ms");
  },

  // ---- onboard profiles -------------------------------------------------------------------

  /** The APPLICATIONS tab only exists in real time. */
  advTabOk() {
    return this.advTab !== "apps" || this.host();
  },

  renderOnboard() {
    const st = this.status;
    $("#ms-onboard-on").checked = !this.host();
    $("#ms-onboard-on").disabled = !st?.connected || !!st?.ghub_running;
    $("#ms-onboard-list").classList.toggle("hidden", this.host());
    const list = st?.profiles || [];
    $("#ms-onboard-list").innerHTML = list.map((p) => {
      const active = p.sector === st.profile;
      const editing = p.sector === this.sector();
      const s = p.settings;
      const summary = s ? `${s.dpi.join(" / ")} DPI · ${s.report_rate_hz} Hz` : "—";
      return `<div class="ms-ob ${editing ? "on" : ""} ${p.enabled ? "" : "off"}" data-sector="${p.sector}">
        <span class="ms-ob-n">${p.sector}</span>
        <div class="ms-ob-id"><b>${esc(t("Profil intégré {n}", { n: p.sector }))}</b>
          ${active ? `<span class="tag def">${esc(t("ACTIF"))}</span>` : ""}
          ${!p.enabled ? `<span class="tag">${esc(t("DÉSACTIVÉ"))}</span>` : ""}
          <small>${esc(summary)}</small></div>
        <span class="grow"></span>
        ${editing ? `<span class="tag live">${esc(t("AFFICHÉ"))}</span>` : `<button class="mini" data-act="show">${esc(t("AFFICHER"))}</button>`}
        ${!active && p.enabled ? `<button class="mini" data-act="use" title="${esc(t("La souris passe sur ce profil"))}">${esc(t("UTILISER"))}</button>` : ""}
        <button class="mini" data-act="${p.enabled ? "disable" : "enable"}" ${active && p.enabled ? "disabled" : ""}
          title="${esc(t("Le bouton de profil de la souris ne passe que par les profils activés"))}">${esc(p.enabled ? t("DÉSACTIVER") : t("ACTIVER"))}</button>
      </div>`;
    }).join("") || `<p class="muted">${esc(t("Souris non détectée"))}</p>`;
  },

  renderAdvanced() {
    $$("#ms-adv-tabs button").forEach((b) => b.classList.toggle("on", b.dataset.tab === this.advTab));
    $('#ms-adv-tabs [data-tab="apps"]').classList.toggle("hidden", !this.host());
    $$("#ms-adv [data-pane]").forEach((p) => p.classList.toggle("hidden", p.dataset.pane !== this.advTab));
    $("#ms-adv-sub").textContent = this.status?.connected ? `${this.status.name} · ${this.profileName()}` : "";
    if (this.advTab === "dpi") this.renderDpi();
    if (this.advTab === "apps") this.renderApps();
    if (this.advTab === "macros") {
      $("#ms-macros-off").classList.toggle("hidden", this.host());
      $("#ms-macros").classList.toggle("hidden", !this.host());
      if (this.host()) this.macroEd.render();
    }
    if (this.advTab === "light") this.renderLighting();
    if (this.advTab === "onboard") this.renderOnboard();
    if (this.advTab === "rate" && this.draft) {
      const saved = this.slotSettings();
      $$("#ms-rate button").forEach((b) => b.classList.toggle("on", +b.dataset.hz === this.draft.report_rate_hz));
      $("#ms-rate-note").classList.toggle("hidden", !saved || this.draft.report_rate_hz === saved.report_rate_hz);
    }
    this.renderFooter();
  },

  // ---- page -------------------------------------------------------------------------------

  renderTab() {
    const st = this.status;
    for (const tab of $$('.dev-tabs button[data-dev="mouse"]')) {
      tab.classList.toggle("off", !st?.connected);
      tab.querySelector(".mouse-tab-name").textContent = st?.connected ? st.name : t("SOURIS");
      tab.querySelector(".mouse-batt").innerHTML = st?.connected && st.battery != null ? BATTERY_ICON(st.battery, st.charging) : "";
      tab.title = st?.connected ? `${st.name} · ${st.battery ?? "?"} %${st.charging ? " · " + t("en charge") : ""}` : t("Souris non détectée");
    }
  },

  render() {
    const st = this.status;
    const page = $("#mouse-page");
    page.classList.toggle("empty", !st?.connected);
    $("#ms-ghub").classList.toggle("hidden", !st?.ghub_running);
    this.renderSoftProfiles();
    if (!st?.connected) {
      $("#ms-name").textContent = t("Souris non détectée");
      $("#ms-state").textContent = "· " + t("Allumez la souris (ou bougez-la pour la réveiller). Le récepteur LIGHTSPEED doit être branché.");
      this.renderFooter();
      return;
    }
    $("#ms-name").textContent = st.name;
    const pct = st.battery ?? 0;
    $("#ms-batt-fill").style.width = pct + "%";
    $("#ms-batt-fill").classList.toggle("low", pct <= 15);
    $("#ms-batt-val").textContent = st.battery != null ? `${pct} %` : "—";
    $("#ms-batt-chg").classList.toggle("hidden", !st.charging);
    const editing = this.sector();
    const soft = this.host() ? this.softIndex() : null;
    $("#ms-state").textContent = "· " + [
      this.host() ? t("temps réel, profil {n}", { n: (st.soft_active ?? 0) + 1 })
        : st.profile ? t("profil interne {n}", { n: st.profile }) : t("pilotée par un logiciel"),
      this.host() ? (soft !== st.soft_active ? t("vous modifiez le profil {n}", { n: soft + 1 }) : "")
        : editing && st.profile && editing !== st.profile ? t("vous modifiez le profil {n}", { n: editing }) : "",
      st.charging ? t("en charge") : "",
    ].filter(Boolean).join(" · ");
    this.renderScope();
    if (typeof OVERLAY !== "undefined" && OVERLAY) renderOverlayHead();
    page.classList.toggle("host", !this.draft);
    this.renderButtons();
    if ($("#ms-adv").classList.contains("open")) this.renderAdvanced();
    this.renderFooter();
  },

  /** Header bar: number and name of the profile shown, what can be done with it. */
  renderBar() {
    const st = this.status, host = this.host();
    const i = this.softIndex();
    $("#ms-pnum").textContent = !st?.connected && !host ? t("PROFIL")
      : host ? t("PROFIL {n}", { n: i + 1 }) : t("PROFIL INTERNE {n}", { n: this.sector() ?? "—" });
    $("#ms-pname").innerHTML = host ? (i === this.softDefault() ? '<span class="star">★</span> ' : "") + esc(this.softName(i))
      : esc(st?.connected ? t("Profil intégré {n}", { n: this.sector() }) : "—");
    const why = t("Pour les profils G Nexus : désactivez les profils intégrés (Paramètres avancés › Profils intégrés)");
    $$(".ms-pbar .ms-host-only").forEach((b) => {
      b.disabled = !host;
      if (!b.dataset.title0) b.dataset.title0 = b.title;
      b.title = host ? b.dataset.title0 : why;
    });
    $("#ms-new").disabled = $("#ms-dup").disabled = !host || (st?.soft_profiles?.length || 0) >= 5;
    $("#ms-delete").disabled = !host || i === this.softDefault();
    if (host && i === this.softDefault()) $("#ms-delete").title = t("Le profil par défaut ne peut pas être supprimé : choisissez-en un autre par défaut d'abord");
    $("#ms-clear").disabled = !this.draft?.buttons;
  },

  renderFooter() {
    this.renderBar();
    const st = this.status;
    const dirty = !!this.draft && this.dirty();
    $$(".ms-save").forEach((b) => {
      b.disabled = !dirty || this.saving || !st?.connected || st.ghub_running;
      b.textContent = this.saving ? t("ENREGISTREMENT…") : this.host() ? t("ENREGISTRER") : t("ENREGISTRER DANS LA SOURIS");
    });
    $$(".ms-revert").forEach((b) => { b.disabled = !dirty || this.saving; });
    const text = st?.ghub_running ? t("Fermez G HUB pour enregistrer.")
      : dirty ? (this.editingActive() ? t("Essai en cours sur la souris, non enregistré") : t("Modifications non enregistrées"))
      : st?.connected && this.draft ? t("Identique à la souris") : "";
    $$(".ms-dirty-text").forEach((e) => { e.textContent = text; });
  },
};
