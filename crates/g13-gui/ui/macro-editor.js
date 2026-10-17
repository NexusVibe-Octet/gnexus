// Macro editor shared by the G13 (Paramètres avancés › Macros) and the mouse
// (real time): a list of macros on the left, the steps of the one chosen on the
// right, and RECORD, which captures the keyboard and the mouse buttons with the
// delays between them (low-level hooks in the back-end).
//
// new MacroEditor({ list, edit, get, set, changed, renamed, deleted })
//   get() / set(obj)   the macros object { name: { mode, steps } } (set: after a rename)
//   changed(msg)       something changed (save, or mark the draft as modified)
//   renamed(old, new)  / deleted(name): update the bindings that use the macro
"use strict";

// The macro library, shared by every G13 and mouse profile (back-end: macros.json).
// A profile keeps its own copy of a macro; the library keeps the latest version.
const MacroLib = {
  items: null,
  loading: null,
  async ensure() {
    if (this.items) return this.items;
    this.loading ??= invoke("macro_library").then((l) => { this.items = l || {}; return this.items; }).catch(() => { this.items = {}; return this.items; });
    return this.loading;
  },
  timers: {},
  /** Remember `name` (debounced: a step being typed is saved once). */
  put(name, m, old = null) {
    if (!this.items || !m) return;
    if (old && old !== name) delete this.items[old];
    this.items[name] = structuredClone(m);
    clearTimeout(this.timers[name]);
    this.timers[name] = setTimeout(() => invoke("save_library_macro", { name, mac: this.items[name], old }).catch(() => {}), old ? 0 : 400);
  },
  remove(name) {
    if (this.items) delete this.items[name];
    invoke("delete_library_macro", { name }).catch(() => {});
  },
  /** Library macros that `profileMacros` does not have yet. */
  others(profileMacros) {
    return Object.keys(this.items || {}).filter((n) => !(profileMacros || {})[n]);
  },
};

class MacroEditor {
  constructor(o) {
    this.o = o;
    this.sel = null;
    this.recording = false;
    o.list.addEventListener("click", async (e) => {
      if (e.target.closest("[data-act=add]")) { this.add(); this.render(); this.changed(t("Macro créée")); return; }
      const lib = e.target.closest(".macro-item.lib");
      if (lib && e.target.closest("[data-act=unlib]")) {
        const ok = await ask(t("RETIRER « {name} » DE LA BIBLIOTHÈQUE ?", { name: lib.dataset.name }), "", { confirmOnly: true, ok: t("RETIRER") });
        if (ok) { MacroLib.remove(lib.dataset.name); this.render(); }
        return;
      }
      if (lib) {
        // copied into this profile, then edited here
        const all = this.o.get();
        if (!all) return;
        all[lib.dataset.name] = structuredClone(MacroLib.items[lib.dataset.name]);
        this.sel = lib.dataset.name;
        this.render();
        this.o.changed(t("Macro « {name} » ajoutée depuis la bibliothèque", { name: lib.dataset.name }));
        return;
      }
      const it = e.target.closest(".macro-item");
      if (it) { this.sel = it.dataset.name; this.render(); }
    });
    o.edit.addEventListener("change", (e) => this.onChange(e));
    o.edit.addEventListener("click", (e) => this.onClick(e));
  }

  get macros() {
    return this.o.get() || {};
  }

  /** New macro "macro-n", selected; returns its name. */
  add() {
    const all = this.o.get();
    let n = 1;
    while (all[`macro-${n}`] || MacroLib.items?.[`macro-${n}`]) n++;
    const name = `macro-${n}`;
    all[name] = { mode: "once", steps: ["down A", "wait 30", "up A"] };
    this.sel = name;
    MacroLib.put(name, all[name]);
    return name;
  }

  /** Something changed: tell the owner, and keep the macro shown in the library. */
  changed(msg) {
    if (this.sel && this.macros[this.sel]) MacroLib.put(this.sel, this.macros[this.sel]);
    this.o.changed(msg);
  }

  render() {
    if (!MacroLib.items) MacroLib.ensure().then(() => this.render());
    const all = this.macros;
    const names = Object.keys(all);
    if (!this.sel || !all[this.sel]) this.sel = names[0] || null;
    const lib = MacroLib.others(all);
    this.o.list.innerHTML = names.map((n) => {
      const mode = t((MACRO_MODES.find(([m]) => m === (all[n].mode || "once")) || [])[1] || "");
      return `<div class="macro-item ${n === this.sel ? "on" : ""}" data-name="${esc(n)}">▶ ${esc(n)}<small>${esc(mode)}</small></div>`;
    }).join("") + `<button class="ghost small" data-act="add">${t("+ NOUVELLE MACRO")}</button>` +
      (lib.length ? `<div class="group-cap lib-cap" title="${esc(t("Macros des autres profils (G13 et souris) : cliquez pour en mettre une copie dans ce profil"))}">${t("BIBLIOTHÈQUE")}</div>` +
        lib.map((n) => `<div class="macro-item lib" data-name="${esc(n)}" title="${esc(t("Ajouter à ce profil"))}">＋ ${esc(n)}
          <button class="unlib" data-act="unlib" title="${esc(t("Retirer de la bibliothèque"))}">×</button><small>${esc(t("{n} étapes", { n: MacroLib.items[n].steps.length }))}</small></div>`).join("") : "");

    const ed = this.o.edit;
    if (!this.sel) {
      ed.innerHTML = `<p class="muted">${t("Aucune macro dans ce profil.")}</p>`;
      return;
    }
    const m = all[this.sel];
    const steps = m.steps.map((s, i) => {
      const [op, ...rest] = s.trim().split(/\s+/);
      const arg = rest.join(" ");
      const kind = (STEP_OPS.find(([o]) => o === op) || [])[2] || "number";
      let argHtml;
      if (kind === "key") argHtml = `<select class="arg">${keyOptions()}</select>`;
      else if (kind === "button") argHtml = `<select class="arg">${btnOptions()}</select>`;
      else argHtml = `<input class="arg" type="number" value="${esc(arg)}">`;
      return `<div class="step" data-i="${i}" data-arg="${esc(kind === "key" ? canonicalKey(arg) : arg)}">
        <span class="n">${i + 1}</span>
        <select class="op">${STEP_OPS.map(([o, l]) => `<option value="${o}" ${o === op ? "selected" : ""}>${t(l)}</option>`).join("")}</select>
        ${argHtml}
        <button class="icon-btn" data-up title="${t("Monter")}"><svg viewBox="0 0 24 24"><path d="m6 15 6-6 6 6"/></svg></button>
        <button class="icon-btn danger" data-del title="${t("Supprimer l'étape")}"><svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6 6 18"/></svg></button>
      </div>`;
    }).join("");
    const rec = this.recording;
    ed.innerHTML = `
      <div class="row"><label>${t("Nom")}</label><input class="macro-name" value="${esc(this.sel)}" spellcheck="false"></div>
      <div class="row"><label>${t("Mode")}</label><select class="macro-mode">${MACRO_MODES.map(([v, l]) =>
        `<option value="${v}" ${v === (m.mode || "once") ? "selected" : ""}>${t(l)}</option>`).join("")}</select></div>
      <div class="rec-row ${rec ? "on" : ""}">
        <button class="rec-btn" data-act="record">${rec ? `<i class="sq"></i>${t("STOP")}` : `<i></i>${t("RECORD")}`}</button>
        ${rec ? `<span class="rec-live">${t("Enregistrement… faites vos actions (clavier, boutons de souris, molette), puis STOP.")}</span>`
          : `<label class="rec-opt"><input type="checkbox" class="rec-delays" ${MacroEditor.delays ? "checked" : ""}> ${t("Garder les délais réels")}</label>
             <span class="muted small-print">${t("RECORD remplace les étapes de cette macro.")}</span>`}
      </div>
      <div class="group-cap">${t("Étapes")}</div>
      <div class="steps">${steps || `<p class="muted small-print">${t("Aucune étape.")}</p>`}</div>
      <div class="row">
        <button class="ghost small" data-act="step-add">${t("+ ÉTAPE")}</button>
        <button class="ghost small" data-act="step-tap">${t("+ FRAPPE DE TOUCHE")}</button>
        <span style="flex:1"></span>
        <button class="ghost small" data-act="del" style="color:var(--danger)">${t("SUPPRIMER LA MACRO")}</button>
      </div>`;
    for (const st of ed.querySelectorAll(".steps .step")) {
      const sel = st.querySelector("select.arg");
      if (sel) sel.value = st.dataset.arg;
    }
  }

  onChange(e) {
    const all = this.macros;
    const m = all[this.sel];
    if (!m) return;
    if (e.target.classList.contains("rec-delays")) { MacroEditor.delays = e.target.checked; return; }
    if (e.target.classList.contains("macro-mode")) { m.mode = e.target.value; this.changed(t("Mode enregistré")); this.render(); return; }
    if (e.target.classList.contains("macro-name")) {
      const nn = e.target.value.trim();
      if (!nn || (nn !== this.sel && all[nn])) { toast(t("Nom vide ou déjà utilisé"), true); e.target.value = this.sel; return; }
      // insertion order kept
      const old = this.sel;
      this.o.set(Object.fromEntries(Object.entries(all).map(([k, v]) => [k === old ? nn : k, v])));
      this.sel = nn;
      this.o.renamed(old, nn);
      MacroLib.put(nn, this.macros[nn], old);
      this.render();
      this.o.changed(t("Macro renommée"));
      return;
    }
    const row = e.target.closest(".step");
    if (!row) return;
    if (e.target.classList.contains("op")) {
      const kind = STEP_OPS.find(([o]) => o === e.target.value)[2];
      const def = kind === "key" ? "A" : kind === "button" ? "Left" : e.target.value === "wait" ? "50" : "1";
      m.steps[+row.dataset.i] = `${e.target.value} ${def}`;
      this.render();
    } else {
      m.steps[+row.dataset.i] = `${row.querySelector(".op").value} ${row.querySelector(".arg").value}`;
    }
    this.changed();
  }

  onClick(e) {
    const m = this.sel && this.macros[this.sel];
    if (!m) return;
    const row = e.target.closest(".step");
    const act = e.target.closest("[data-act]")?.dataset.act;
    if (act === "record") { this.toggleRecord(); return; }
    if (e.target.closest("[data-del]") && row) m.steps.splice(+row.dataset.i, 1);
    else if (e.target.closest("[data-up]") && row) {
      const i = +row.dataset.i;
      if (i > 0) [m.steps[i - 1], m.steps[i]] = [m.steps[i], m.steps[i - 1]];
    } else if (act === "step-add") m.steps.push("wait 50");
    else if (act === "step-tap") m.steps.push("down A", "wait 30", "up A", "wait 30");
    else if (act === "del") {
      const name = this.sel;
      delete this.macros[name];
      this.sel = null;
      this.o.deleted(name);
    } else return;
    this.render();
    this.changed();
  }

  /** RECORD / STOP. The click on STOP itself is left out of the macro. */
  async toggleRecord() {
    if (!this.recording) {
      try {
        await invoke("record_start");
        this.recording = true;
        this.render();
      } catch (e) { toast(t("Enregistrement impossible : {e}", { e }), true); }
      return;
    }
    this.recording = false;
    try {
      const steps = await invoke("record_stop", { delays: !!MacroEditor.delays });
      const m = this.macros[this.sel];
      if (m && steps.length) {
        m.steps = steps;
        this.changed(t("{n} étapes enregistrées", { n: steps.length }));
      } else if (!steps.length) toast(t("Rien n'a été enregistré"));
    } catch (e) { toast(String(e), true); }
    this.render();
  }

  /** Leaving the editor while recording: stop and drop the capture. */
  abort() {
    if (!this.recording) return;
    this.recording = false;
    invoke("record_stop", { delays: false }).catch(() => {});
  }
}
MacroEditor.delays = true;

// built on use: the labels follow the chosen layout and language
const keyOptions = () => ALL_CODES.map((c) => `<option value="${c}">${esc(KEY_LABELS[c])}  (${c})</option>`).join("");
const btnOptions = () => ["Left", "Right", "Middle", "Back", "Forward"]
  .map((b) => `<option value="${b}">${esc(t(MOUSE.find(([o]) => o.mouse === b)[2]))}</option>`).join("");
