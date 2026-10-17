// Headset page (PRO X Wireless…): a blueprint of the headset, its battery, volume
// and power-off delay, and G Nexus's equalizer with its profiles: white pills on
// the left (as for the G13 and the mouse), the default one ★ everywhere, the others
// in the applications they are bound to (in the foreground). The profile in use is
// shown on the G logo of the right ear cup. Every change is heard at once.
//
// showDevice(dev) switches the whole window between the G13, the mouse and
// the headset (tabs in the top bar, and in the overlay's head).
"use strict";

/** Show the G13 ("g13"), the mouse ("mouse") or the headset ("headset"). */
function showDevice(dev) {
  document.body.classList.toggle("mouse-mode", dev === "mouse");
  document.body.classList.toggle("headset-mode", dev === "headset");
  $$(".dev-tabs button").forEach((b) => b.classList.toggle("on", b.dataset.dev === dev));
  if (dev === "mouse") Mouse.render();
  if (dev === "headset") Headset.render();
  if (typeof OVERLAY !== "undefined" && OVERLAY) renderOverlayHead();
}

// equalizer presets (10 bands: 32 Hz … 16 kHz)
const EQ_PRESETS = [
  ["flat", "PLAT", [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]],
  ["bass", "BASSES +", [6, 5, 4, 2, 0, 0, 0, 0, 0, 0]],
  ["fps", "FPS (PAS)", [-4, -3, -2, -1, 0, 2, 4, 5, 3, 0]],
  ["voice", "VOIX", [-3, -2, -1, 1, 3, 4, 4, 2, 0, -1]],
  ["music", "MUSIQUE", [4, 3, 2, 0, -1, 0, 1, 2, 3, 3]],
  ["movie", "CINÉMA", [5, 4, 2, 0, -1, 0, 1, 2, 2, 1]],
];
const EQ_MAX_PROFILES = 8;

const Headset = {
  status: null,
  eqTimer: null,
  eq: null, // gains of the profile shown (sent after a short pause)
  editEq: null, // equalizer profile shown (null = the one in use)
  pendingEq: null, // a profile just added: shown once it exists

  init() {
    this.buildDrawing();
    // Windows volume of the headset: live while dragging, like the system mixer
    $("#hs-volume").addEventListener("input", (e) => {
      $("#hs-volume-val").textContent = `${e.target.value} %`;
      clearTimeout(this.volTimer);
      this.volTimer = setTimeout(() => this.send("set_headset_volume", { volume: +e.target.value }), 40);
    });
    $("#hs-mute").addEventListener("click", () => this.send("set_headset_mute", { mute: !this.status?.muted }));
    $("#hs-sidetone").addEventListener("input", (e) => { $("#hs-sidetone-val").textContent = `${e.target.value} %`; });
    $("#hs-sidetone").addEventListener("change", (e) => this.send("set_headset_sidetone", { level: +e.target.value }));
    $("#hs-sleep").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-min]");
      if (b) this.send("set_headset_sleep", { minutes: +b.dataset.min });
    });
    $("#hs-presets").addEventListener("click", (e) => {
      const b = e.target.closest("button[data-preset]");
      const p = b && EQ_PRESETS.find(([id]) => id === b.dataset.preset);
      if (!p || !this.eq) return;
      this.eq = p[2].slice(0, this.eq.length);
      this.renderEq();
      this.sendEq(true);
    });
    $("#hs-eq").addEventListener("input", (e) => {
      const i = e.target.dataset?.band;
      if (i == null || !this.eq) return;
      this.eq[+i] = +e.target.value;
      e.target.closest(".eq-band").querySelector("b").textContent = this.fmtDb(+e.target.value);
      this.renderPresets();
    });
    $("#hs-eq").addEventListener("change", () => this.sendEq());
    $("#hs-eq-install").addEventListener("click", () => this.setupEq("audio_eq_install", t("Égaliseur installé : réglez-le, l'effet est immédiat")));
    $("#hs-eq-reinstall").addEventListener("click", () => this.setupEq("audio_eq_install", t("Égaliseur réinstallé")));
    $("#hs-eq-uninstall").addEventListener("click", async () => {
      const ok = await ask(t("DÉSINSTALLER L'ÉGALISEUR ?"), "", { confirmOnly: true, ok: t("DÉSINSTALLER") });
      if (ok) this.setupEq("audio_eq_uninstall", t("Égaliseur désinstallé"));
    });
    // equalizer profiles: pills, header bar, scope line, applications
    $("#hs-eqp").addEventListener("click", (e) => {
      const b = e.target.closest("button.bank");
      if (!b || b.disabled) return;
      if (b.dataset.add != null) this.newEq(false);
      else this.showEq(+b.dataset.i);
    });
    $("#hs-new").addEventListener("click", () => this.newEq(false));
    $("#hs-dup").addEventListener("click", () => this.newEq(true));
    $("#hs-rename").addEventListener("click", async () => {
      const p = this.eqProfile();
      if (!p) return;
      const name = await ask(t("RENOMMER LE PROFIL"), this.eqName(this.eqIndex()));
      if (name) this.saveEq({ ...p, name });
    });
    $("#hs-delete").addEventListener("click", async () => {
      const i = this.eqIndex();
      if (i === this.status?.eq_default) return;
      const ok = await ask(t("SUPPRIMER « {name} » ?", { name: this.eqName(i) }), "", { confirmOnly: true, ok: t("SUPPRIMER") });
      if (!ok) return;
      this.editEq = null;
      this.send("delete_eq_profile", { index: i });
    });
    $("#hs-pin").addEventListener("click", () => {
      const i = this.eqIndex();
      const pinned = this.status?.eq_pinned === i ? null : i;
      this.send("pin_eq_profile", { index: pinned });
      toast(pinned != null ? t("Profil d'égaliseur actif partout jusqu'à l'arrêt du test") : t("Test arrêté : bascule automatique rétablie"));
    });
    $("#hs-default").addEventListener("click", () => {
      this.send("set_eq_default", { index: this.eqIndex() });
      toast(t("Profil par défaut : il s'applique maintenant partout"));
    });
    $("#hs-assign").addEventListener("click", () => {
      $("#hs-eq-apps").open = true;
      this.renderApps();
    });
    $("#hs-eq-apps").addEventListener("toggle", () => { if ($("#hs-eq-apps").open) this.renderApps(); });
    $("#hs-apps-bound").addEventListener("click", (e) => {
      const b = e.target.closest("[data-exe]");
      const p = this.eqProfile();
      if (b && p) this.saveEq({ ...p, apps: p.apps.filter((a) => a !== b.dataset.exe) }, false);
    });
    $("#hs-apps-detected").addEventListener("click", (e) => {
      const b = e.target.closest(".app-chip");
      if (b && !b.disabled) this.bindApp(b.dataset.exe);
    });
    $("#hs-apps-browse").addEventListener("click", async () => {
      try { const exe = await invoke("pick_exe"); if (exe) this.bindApp(exe); } catch (x) { toast(String(x), true); }
    });
    $$(".hs-eq-export").forEach((b) => b.addEventListener("click", async () => {
      const p = this.eqProfile();
      if (!p) { toast(t("Casque non détecté"), true); return; }
      try {
        const path = await invoke("export_eq_profile", { profile: { ...p, name: this.eqName(this.eqIndex()) } });
        if (path) toast(t("Profil d'égaliseur exporté : {path}", { path }));
      } catch (e) { toast(t("Export impossible : {e}", { e }), true); }
    }));
    $$(".hs-eq-import").forEach((b) => b.addEventListener("click", async () => {
      if (!this.status?.eq_profiles) { toast(t("Casque non détecté"), true); return; }
      if (this.status.eq_profiles.length >= EQ_MAX_PROFILES) { toast(t("{n} profils au plus : supprimez-en un d'abord", { n: EQ_MAX_PROFILES }), true); return; }
      try {
        const p = await invoke("import_eq_profile");
        if (!p) return;
        // added as a new profile, bound to no application (yours may differ)
        this.pendingEq = this.status.eq_profiles.length;
        this.send("save_eq_profile", { index: null, profile: { ...p, apps: [] } });
        closeSheets();
        showDevice("headset");
        toast(t("Profil d'égaliseur importé : « {name} »", { name: p.name || t("sans nom") }));
      } catch (e) { toast(t("Import impossible : {e}", { e }), true); }
    }));
    invoke("get_headset").then((st) => { if (st) this.update(st); }).catch(() => {});
    this.renderTab();
  },

  fmtDb(v) {
    return (v > 0 ? "+" : "") + v;
  },

  send(cmd, args) {
    invoke(cmd, args).catch((e) => toast(String(e), true));
  },

  // ---- equalizer profiles ---------------------------------------------------------

  /** Profile shown (the one in use unless another pill was chosen). */
  eqIndex() {
    const n = this.status?.eq_profiles?.length || 0;
    return Math.min(this.editEq ?? this.status?.eq_active ?? 0, Math.max(0, n - 1));
  },
  eqProfile() {
    return this.status?.eq_profiles?.[this.eqIndex()] ?? null;
  },
  eqName(i) {
    return this.status?.eq_profiles?.[i]?.name || t("Profil {n}", { n: i + 1 });
  },

  /** Hear profile `i` and show it. */
  showEq(i) {
    this.editEq = i;
    this.eq = this.status?.eq_profiles?.[i]?.gains.slice() ?? this.eq;
    this.send("use_eq_profile", { index: i });
    this.render();
  },

  /** Save the profile shown (`apply`: it is heard at once, which the back-end always does). */
  saveEq(profile) {
    this.send("save_eq_profile", { index: this.eqIndex(), profile });
  },

  async newEq(copy) {
    const st = this.status;
    if (!st || (st.eq_profiles?.length || 0) >= EQ_MAX_PROFILES) { toast(t("{n} profils au plus : supprimez-en un d'abord", { n: EQ_MAX_PROFILES }), true); return; }
    const name = await ask(copy ? t("DUPLIQUER LE PROFIL") : t("NOUVEAU PROFIL"),
      copy ? t("{name} (copie)", { name: this.eqName(this.eqIndex()) }) : "", { placeholder: t("nom du jeu ou de l'application") });
    if (name == null) return;
    const gains = copy ? (this.eq || []).slice() : EQ_PRESETS[0][2].slice();
    this.pendingEq = st.eq_profiles.length;
    this.send("save_eq_profile", { index: null, profile: { name: name || "", apps: [], gains } });
  },

  bindApp(exe) {
    const p = this.eqProfile();
    if (!p) return;
    const lower = exe.toLowerCase();
    // an application has one equalizer profile: taken from another one if needed
    (this.status.eq_profiles || []).forEach((o, i) => {
      if (i !== this.eqIndex() && o.apps.some((a) => a.toLowerCase() === lower)) {
        invoke("save_eq_profile", { index: i, profile: { ...o, apps: o.apps.filter((a) => a.toLowerCase() !== lower) } }).catch(() => {});
      }
    });
    if (!p.apps.some((a) => a.toLowerCase() === lower)) this.saveEq({ ...p, apps: [...p.apps, exe] });
    toast(t("« {name} » s'activera désormais dans {app}", { name: this.eqName(this.eqIndex()), app: exe }));
  },

  sendEq(now = false) {
    clearTimeout(this.eqTimer);
    const go = () => {
      const p = this.eqProfile();
      if (p && this.eq) this.saveEq({ ...p, gains: this.eq.slice() });
    };
    if (now) go(); else this.eqTimer = setTimeout(go, 150);
  },

  /** Install / remove the equalizer effect (Windows asks for administrator rights; Linux: a PipeWire filter, no rights). */
  async setupEq(cmd, done) {
    const b = $("#hs-eq-install");
    b.disabled = true;
    b.textContent = t("INSTALLATION…");
    try {
      await invoke(cmd);
      toast(done);
    } catch (e) {
      toast(t("Égaliseur : {e}", { e }), true);
    }
    b.disabled = false;
    b.textContent = t("INSTALLER L'ÉGALISEUR");
  },

  update(st) {
    const volOnly = this.status && JSON.stringify({ ...st, volume: 0, muted: false }) === JSON.stringify({ ...this.status, volume: 0, muted: false });
    this.status = st;
    // the volume moves often: only the volume row is redrawn then
    if (volOnly) { if (document.body.classList.contains("headset-mode")) this.renderVolume(); return; }
    const n = st.eq_profiles?.length || 0;
    if (this.pendingEq != null && n > this.pendingEq) { this.editEq = this.pendingEq; this.pendingEq = null; }
    if (this.editEq != null && this.editEq >= n) this.editEq = null;
    // the gains of the profile shown, unless a slider is being dragged
    if (!document.activeElement?.matches?.("#hs-eq input")) this.eq = this.eqProfile()?.gains.slice() ?? null;
    if (st.message && st.error) toast(t(st.message), true);
    this.renderTab();
    if (document.body.classList.contains("headset-mode")) this.render();
  },

  renderTab() {
    const st = this.status;
    for (const tab of $$('.dev-tabs button[data-dev="headset"]')) {
      tab.classList.toggle("off", !st?.connected);
      tab.querySelector(".headset-tab-name").textContent = st?.present ? st.name : t("CASQUE");
      tab.querySelector(".headset-batt").innerHTML = st?.connected && st.battery != null ? BATTERY_ICON(st.battery, st.charging) : "";
      tab.title = !st?.present ? t("Casque non détecté") : !st.connected ? t("Casque éteint")
        : `${st.name} · ${st.battery ?? "?"} %${st.charging ? " · " + t("en charge") : ""}`;
    }
  },

  render() {
    const st = this.status;
    const page = $("#headset-page");
    page.classList.toggle("empty", !st?.connected);
    $("#hs-name").textContent = st?.present ? st.name : t("Casque non détecté");
    $("#hs-state").textContent = "· " + (!st?.present ? t("Branchez le récepteur USB du casque.")
      : !st.connected ? t("Allumez le casque (le récepteur est branché).")
      : [t("allumé"), st.charging ? t("en charge") : "", st.voltage_mv ? `${(st.voltage_mv / 1000).toFixed(2)} V` : ""].filter(Boolean).join(" · "));
    const pct = st?.battery ?? 0;
    $("#hs-batt-fill").style.width = pct + "%";
    $("#hs-batt-fill").classList.toggle("low", pct <= 15);
    $("#hs-batt-val").textContent = st?.connected && st.battery != null ? `${pct} %` : "—";
    $("#hs-batt-chg").classList.toggle("hidden", !st?.charging);
    this.renderVolume();
    // sidetone: not while it is being dragged
    if (st?.sidetone != null && document.activeElement !== $("#hs-sidetone")) {
      $("#hs-sidetone").value = st.sidetone;
      $("#hs-sidetone-val").textContent = `${st.sidetone} %`;
    }
    $("#hs-sleep").innerHTML = [0, 5, 10, 15, 30, 60, 90].map((m) =>
      `<button data-min="${m}" class="${st?.sleep_minutes === m ? "on" : ""}">${m ? t("{n} min", { n: m }) : t("JAMAIS")}</button>`).join("");
    this.renderEqProfiles();
    this.renderEq();
    if ($("#hs-eq-apps").open) this.renderApps();
  },

  /** Volume row (also redrawn alone when only the volume moved). */
  renderVolume() {
    const st = this.status;
    const vol = st?.volume;
    $("#hs-vol-dev").textContent = st?.audio_device
      || (ON_LINUX ? t("Sortie audio du casque introuvable dans PipeWire.") : t("Sortie audio du casque introuvable dans Windows."));
    if (vol != null && document.activeElement !== $("#hs-volume")) {
      $("#hs-volume").value = vol;
      $("#hs-volume-val").textContent = `${vol} %`;
    }
    $("#hs-volume").disabled = vol == null;
    $("#hs-mute").classList.toggle("muted", !!st?.muted);
    $("#hs-mute").classList.toggle("lvl1", vol != null && vol < 50);
  },

  /** Pills, header bar, scope line and the label on the drawing's G logo. */
  renderEqProfiles() {
    const st = this.status;
    const list = st?.eq_profiles || [];
    const cur = this.eqIndex(), def = st?.eq_default ?? 0, active = st?.eq_active ?? 0;
    const html = list.map((p, i) => {
      const apps = p.apps.length ? p.apps.join(", ") : i === def ? t("toutes les autres applications") : t("aucune application");
      return `<button class="bank prof ${i === cur ? "on" : ""}" data-i="${i}" title="${esc(this.eqName(i) + " · " + apps)}">${i + 1}${i === active ? '<i class="act"></i>' : ""}</button>`;
    }).join("") + (list.length < EQ_MAX_PROFILES ? `<button class="bank add" data-add title="${esc(t("Nouveau profil"))}">+</button>` : "");
    // rebuilt only when it changes: a click must not lose its button
    if (html !== this.pillsHtml) { $("#hs-eqp").innerHTML = html; this.pillsHtml = html; }
    $("#hs-eqp-label").textContent = list.length ? t("PROFIL {n}", { n: cur + 1 }) : "";
    // header bar
    $("#hs-pnum").textContent = t("PROFIL {n}", { n: cur + 1 });
    $("#hs-pname").innerHTML = (cur === def ? '<span class="star">★</span> ' : "") + esc(list.length ? this.eqName(cur) : "—");
    $("#hs-new").disabled = $("#hs-dup").disabled = list.length >= EQ_MAX_PROFILES;
    $("#hs-delete").disabled = cur === def;
    $("#hs-delete").title = cur === def ? t("Le profil par défaut ne peut pas être supprimé : choisissez-en un autre par défaut d'abord") : t("Supprimer le profil");
    // where the profile shown applies
    const p = list[cur];
    const apps = p?.apps || [];
    const pinned = st?.eq_pinned;
    let html2;
    if (pinned != null) {
      html2 = t("Test en cours : « <b>{name}</b> » s'applique au casque <b>dans toutes les applications</b> jusqu'à « Arrêter le test ».", { name: esc(this.eqName(pinned)) });
    } else if (apps.length) {
      html2 = t("« <b>{name}</b> » s'active tout seul dans : <b>{apps}</b>.", { name: esc(this.eqName(cur)), apps: esc(apps.join(", ")) }) +
        (cur === def ? " " + t("Partout ailleurs aussi (profil par défaut ★).") : "");
    } else if (cur === def) {
      html2 = t("« <b>{name}</b> » est le profil par défaut ★ : il s'applique partout où aucun profil de jeu n'est associé.", { name: esc(this.eqName(cur)) });
    } else {
      html2 = t("« <b>{name}</b> » n'est associé à aucun jeu : c'est « <b>{def}</b> » ★ qui s'applique partout.", { name: esc(this.eqName(cur)), def: esc(this.eqName(def)) });
    }
    $("#hs-eq-scope-text").innerHTML = html2;
    $("#hs-eq-scope").classList.toggle("warn", pinned != null ? pinned !== cur : !apps.length && cur !== def);
    $("#hs-pin").textContent = pinned === cur ? t("ARRÊTER LE TEST") : t("TESTER PARTOUT");
    $("#hs-pin").classList.toggle("primary", pinned === cur);
    $("#hs-default").classList.toggle("hidden", cur === def);
    // the G logo of the right ear cup: the profile in use
    const lbl = $("#hdev-eq");
    if (lbl) lbl.textContent = list.length ? (active === def ? "★ " : "") + this.eqName(active) : "—";
  },

  async renderApps() {
    const p = this.eqProfile();
    if (!p) return;
    const i = this.eqIndex(), def = this.status?.eq_default ?? 0;
    $("#hs-apps-bound").innerHTML = p.apps.map((a) => `<button class="app-chip bound" data-exe="${esc(a)}" title="${esc(t("Retirer"))}"><b>${esc(a)}</b><em>${esc(t("retirer"))} ×</em></button>`).join("")
      || `<span class="muted" style="font-size:13px">${esc(i === def ? t("Toutes les autres applications.") : t("Aucune application associée."))}</span>`;
    let list;
    try { list = await invoke("detected_apps"); } catch { list = { running: [], recent: [] }; }
    const seen = new Set();
    const all = [...list.running, ...list.recent].filter((a) => !seen.has(a.exe.toLowerCase()) && seen.add(a.exe.toLowerCase()));
    const owner = (exe) => (this.status?.eq_profiles || []).findIndex((o) => o.apps.some((a) => a.toLowerCase() === exe.toLowerCase()));
    $("#hs-apps-detected").innerHTML = all.map((a) => {
      const o = owner(a.exe);
      return `<button class="app-chip ${o === i ? "bound" : ""}" data-exe="${esc(a.exe)}" ${o === i ? "disabled" : ""} title="${esc(a.title || "")}">
        <b>${esc(a.exe)}</b><span>${esc(a.title || "")}</span>${o >= 0 && o !== i ? `<em>${esc(t("actuellement : {name}", { name: this.eqName(o) }))}</em>` : ""}</button>`;
    }).join("") || `<span class="muted" style="font-size:13px">${esc(t("Aucune application ouverte détectée."))}</span>`;
  },

  renderEq() {
    const eq = this.status?.equalizer;
    const box = $("#hs-eq");
    const installed = !!this.status?.eq_installed;
    $("#hs-eq-setup").classList.toggle("hidden", installed || !this.status?.present);
    for (const id of ["#hs-eq-uninstall", "#hs-eq-reinstall", "#hs-eq-help", "#hs-eq-scope", "#hs-eq-apps"]) $(id).classList.toggle("hidden", !installed);
    box.classList.toggle("hidden", !installed);
    $("#hs-presets").classList.toggle("hidden", !installed);
    $(".hs-rail").classList.toggle("locked", !installed);
    if (!eq || !this.eq) {
      box.innerHTML = "";
      box.dataset.bands = "";
      $("#hs-presets").innerHTML = "";
      return;
    }
    const hz = (f) => (f >= 10000 ? `${Math.round(f / 1000)}k` : f >= 1000 ? `${(f / 1000).toFixed(1).replace(/\.0$/, "")}k` : `${f}`);
    // the sliders are rebuilt only when the bands change (a drag must not be broken)
    if (box.dataset.bands !== eq.freqs.join(",")) {
      box.dataset.bands = eq.freqs.join(",");
      box.innerHTML = `<div class="eq-scale"><span>+${eq.max_db}</span><span>0</span><span>${eq.min_db}</span></div>` +
        eq.freqs.map((f, i) => `<div class="eq-band"><b></b>
          <input type="range" data-band="${i}" min="${eq.min_db}" max="${eq.max_db}" step="1">
          <span>${hz(f)}</span></div>`).join("");
    }
    box.querySelectorAll("input[data-band]").forEach((r) => {
      const v = this.eq[+r.dataset.band] ?? 0;
      if (document.activeElement !== r) r.value = v;
      r.closest(".eq-band").querySelector("b").textContent = this.fmtDb(+r.value);
    });
    this.renderPresets();
  },

  renderPresets() {
    const cur = this.eq || [];
    const on = EQ_PRESETS.find(([, , g]) => g.every((v, i) => v === (cur[i] ?? 0)));
    $("#hs-presets").innerHTML = EQ_PRESETS.map(([id, label]) => `<button data-preset="${id}" class="${on && on[0] === id ? "on" : ""}">${t(label)}</button>`).join("") +
      (on ? "" : `<button class="on" disabled>${t("PERSONNALISÉ")}</button>`);
  },

  /** The headset after the user's Gemini blueprint (its own coordinates), 3/4 view; the
   *  G disc of the right ear cup shows the equalizer profile in use. */
  buildDrawing() {
    const call = (from, to, anchor, id, label, labelId = "") => {
      const end = [to[0] + (anchor === "end" ? 6 : -6), to[1] - 4];
      return `<path class="leader" d="M${from[0]} ${from[1]} L${end[0]} ${end[1]}"/><circle class="leader-dot" cx="${from[0]}" cy="${from[1]}" r="2.2"/>
        <text class="kid" x="${to[0]}" y="${to[1] - 14}" text-anchor="${anchor}">${id}</text>
        <text class="hs-label" ${labelId ? `id="${labelId}"` : ""} x="${to[0]}" y="${to[1] + 4}" text-anchor="${anchor}">${esc(t(label))}</text>`;
    };
    const G = "M 22 4 A 18 18 0 1 0 22 40 A 18 18 0 0 0 38 26 L 38 20 L 22 20 L 22 27 L 30 27 A 10 10 0 1 1 22 12 Z";
    const defs = (id) => `<defs>
      <pattern id="foam-${id}" width="6" height="6" patternTransform="rotate(45)" patternUnits="userSpaceOnUse"><path d="M0 0 V6" class="hatch"/></pattern>
      <pattern id="metal-${id}" width="5" height="5" patternTransform="rotate(-30)" patternUnits="userSpaceOnUse"><path d="M0 0 V5" class="hatch"/></pattern></defs>`;
    const hero = `<svg class="hs-view" data-view="hero" viewBox="60 222 600 570">${defs("h")}
      <path class="shell hs-body" d="M 390.3 261.4 Q 370.6 248.8 346.7 241.2 Q 322.7 233.7 302.6 232.8 Q 282.4 232.0 267.7 234.5 Q 253.0 237.0 223.2 252.6 Q 193.4 268.1 174.1 289.5 Q 154.7 310.9 144.2 332.4 Q 133.7 353.8 128.3 355.0 Q 122.8 356.3 121.5 362.2 Q 120.3 368.1 116.5 391.2 Q 112.7 414.3 114.8 416.8 Q 116.9 419.3 127.0 421.4 Q 137.1 423.5 137.9 426.9 Q 138.8 430.2 136.2 442.4 Q 133.7 454.6 131.6 456.7 Q 129.5 458.8 119.4 467.6 Q 109.4 476.4 103.5 492.0 Q 97.6 507.5 97.6 526.0 Q 97.6 544.5 103.1 562.1 Q 108.5 579.8 106.4 584.8 Q 104.3 589.8 107.7 597.8 Q 111.0 605.8 116.5 604.6 Q 122.0 603.3 122.4 606.2 Q 122.8 609.2 135.8 634.0 Q 148.8 658.8 164.8 674.7 Q 180.8 690.7 205.5 701.2 Q 230.3 711.7 248.4 709.6 Q 266.4 707.5 276.5 702.9 Q 286.6 698.2 287.4 700.8 Q 288.3 703.3 291.6 705.4 Q 295.0 707.5 289.1 713.4 Q 283.2 719.2 263.1 733.9 Q 242.9 748.6 218.6 756.6 Q 194.2 764.6 172.8 764.2 Q 151.4 763.8 148.4 759.6 Q 145.5 755.4 134.6 749.5 Q 123.6 743.6 110.2 743.2 Q 96.8 742.8 90.9 747.4 Q 85.0 752.0 85.0 759.6 Q 85.0 767.1 93.0 773.4 Q 101.0 779.7 119.4 779.7 Q 137.9 779.7 144.6 774.6 Q 151.4 769.6 177.0 769.6 Q 202.6 769.6 227.4 760.8 Q 252.2 752.0 274.0 736.9 Q 295.8 721.8 300.5 717.1 Q 305.1 712.5 307.6 713.4 Q 310.1 714.2 315.2 707.1 Q 320.2 699.9 326.5 702.4 Q 332.8 705.0 347.9 704.1 Q 363.0 703.3 372.3 710.0 Q 381.5 716.7 399.6 715.0 Q 417.6 713.4 439.9 697.8 Q 462.2 682.3 471.4 657.5 Q 480.6 632.7 483.6 607.5 Q 486.5 582.3 490.7 560.5 Q 494.9 538.6 494.9 520.1 Q 494.9 501.6 486.9 485.7 Q 479.0 469.7 468.9 462.6 Q 458.8 455.4 459.2 444.9 Q 459.6 434.4 467.6 431.9 Q 475.6 429.4 475.6 421.8 Q 475.6 414.3 466.0 379.0 Q 456.3 343.7 451.7 343.7 Q 447.1 343.7 439.9 327.8 Q 432.8 311.8 410.1 286.6 Q 387.4 261.4 390.3 261.4 Z M 291.6 279.9 Q 300.9 276.5 321.0 287.9 Q 341.2 299.2 354.2 313.1 Q 367.2 326.9 378.2 345.4 Q 389.1 363.9 388.7 365.6 Q 388.2 367.2 385.3 368.1 Q 382.4 368.9 389.1 397.1 Q 395.8 425.2 399.6 425.6 Q 403.4 426.0 404.6 429.8 Q 405.9 433.6 404.2 438.6 Q 402.5 443.7 399.6 446.6 Q 396.6 449.6 382.4 447.5 Q 368.1 445.4 355.5 451.7 Q 342.9 458.0 326.1 478.5 Q 309.3 499.1 304.2 511.7 Q 299.2 524.3 295.0 521.4 Q 290.8 518.4 275.7 492.4 Q 260.6 466.4 244.6 456.7 Q 228.6 447.0 206.4 448.3 Q 184.1 449.6 177.0 455.4 Q 169.8 461.3 169.8 452.5 Q 169.8 443.7 177.0 439.9 Q 184.1 436.1 189.6 433.6 Q 195.0 431.1 198.8 399.6 Q 202.6 368.1 198.4 366.8 Q 194.2 365.6 205.5 342.5 Q 216.9 319.4 235.4 303.4 Q 253.8 287.4 268.1 283.2 Q 282.4 279.0 291.6 279.9 Z M 135.0 482.3 Q 137.9 480.6 138.3 481.1 Q 138.8 481.5 137.1 484.8 Q 135.4 488.2 132.5 492.0 Q 129.5 495.8 126.2 503.3 Q 122.8 510.9 121.5 517.6 Q 120.3 524.3 119.9 531.5 Q 119.4 538.6 117.8 531.5 Q 116.1 524.3 116.1 518.9 Q 116.1 513.4 116.9 509.2 Q 117.8 505.0 121.1 498.3 Q 124.5 491.6 128.3 487.8 Q 132.0 484.0 135.0 482.3 Z M 459.2 484.4 Q 459.6 483.2 464.3 487.4 Q 468.9 491.6 471.4 496.6 Q 473.9 501.6 473.9 502.9 Q 473.9 504.2 474.3 504.6 Q 474.8 505.0 474.8 513.8 Q 474.8 522.6 473.9 523.9 Q 473.1 525.2 472.7 524.7 Q 472.2 524.3 472.2 521.8 Q 472.2 519.3 471.8 518.9 Q 471.4 518.4 470.6 513.4 Q 469.7 508.4 466.4 501.2 Q 463.0 494.1 461.7 492.4 Q 460.5 490.7 459.6 488.2 Q 458.8 485.7 459.2 484.4 Z"/>
      <!-- headband: outer arch, stitching, inner pad -->
      <path class="seam" d="M 142 350 Q 176 262 276 242 Q 374 238 432 344"/>
      <path class="seam hs-stitch" d="M 152 352 Q 188 268 278 250 Q 366 248 418 346"/>
      <path class="seam" d="M 192 365 Q 225 292 288 279 Q 352 282 388 364"/>
      <!-- sliders -->
      <path class="deco" d="M 122 350 L 198 362 L 190 422 L 115 412 Z"/>
      <path class="deco" d="M 386 350 L 458 342 L 476 416 L 398 424 Z"/>
      <text class="hs-tag" x="431" y="388" text-anchor="middle" transform="rotate(-7 431 388)">PRO</text>
      <path class="seam" d="M 142 416 L 138 442 M 162 419 L 158 442 M 140 424 L 160 426 M 139 432 L 159 434 M 432 421 L 434 446 M 454 419 L 456 444"/>
      <!-- left yoke and pivot, left ear pad (behind) -->
      <path class="inner" d="M 136 442 L 164 444 Q 132 465 114 504 Q 106 535 120 582 L 134 578 Q 120 535 128 504 Q 142 475 170 462"/>
      <ellipse class="deco" cx="123" cy="586" rx="10" ry="15" transform="rotate(-12 123 586)"/>
      <path class="seam" d="M 185 450 Q 146 492 162 585 Q 184 678 242 702 Q 288 708 308 665"/>
      <path class="grip" fill="url(#foam-h)" d="M 218 442 Q 172 476 188 578 Q 206 672 262 698 Q 312 702 325 635 Q 332 548 292 475 Q 258 435 218 442 Z"/>
      <ellipse class="deco" cx="264" cy="568" rx="32" ry="74" transform="rotate(-16 264 568)"/>
      <!-- right ear pad and shell (in front), right yoke -->
      <path class="grip" fill="url(#foam-h)" d="M 446 452 L 426 440 Q 412 446 399.6 446.6 Q 396.6 449.6 382.4 447.5 Q 368.1 445.4 355.5 451.7 Q 342.9 458.0 326.1 478.5 Q 309.3 499.1 304.2 511.7 Q 299.2 524.3 295.0 521.4 Q 284 610 325 682 Q 348 702 375 685 Q 340 625 368 542 Q 400 468 446 452 Z"/>
      <!-- ear cup shell: its inner edge is the cushion's -->
      <path class="shell" d="M 446 452 Q 400 468 368 542 Q 340 625 375 685 Q 412 712 452 678 Q 492 632 486 542 Q 480 472 446 452 Z"/>
      <path class="deco" d="M 428 422 L 458 420 L 462 444 Q 494 468 494 512 Q 492 552 476 588 L 468 576 Q 480 542 478 508 Q 476 472 448 456 Q 405 462 368 518 L 338 594 L 322 588 Q 355 505 398 462 L 426 444 Z"/>
      <ellipse class="deco" cx="326" cy="596" rx="11" ry="16" transform="rotate(14 326 596)"/>
      <!-- the brushed aluminium disc with the G: the equalizer profile in use -->
      <ellipse class="grip" fill="url(#metal-h)" cx="431" cy="598" rx="44" ry="62" transform="rotate(16 431 598)"/>
      <ellipse class="hs-ring" cx="431" cy="598" rx="41" ry="58" transform="rotate(16 431 598)"/>
      <path class="hs-g" transform="translate(416 581) rotate(14) scale(.75)" d="${G}"/>
      <!-- detachable mic -->
      <polygon class="deco" points="315,665 332,676 308,712 290,700"/>
      <path class="hs-boom" d="M 296 706 Q 238 766 150 760"/>
      <ellipse class="grip" fill="url(#foam-h)" cx="118" cy="757" rx="34" ry="22" transform="rotate(8 118 757)"/>
      ${call([476, 616], [530, 640], "start", t("ÉGALISEUR"), "—", "hdev-eq")}

    </svg>`;
    $("#hdev").innerHTML = `<div class="mdev-stage">${hero}</div>`;
  },
};
