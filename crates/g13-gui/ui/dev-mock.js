// Fake Tauri back-end, loaded only when the page is opened in a plain browser
// (UI development without the app or the G13). Not used by the real app.
"use strict";
(() => {
  const profiles = {
    demo: {
      name: "Démo", default: true, rules: [],
      banks: {
        M1: { color: "#FF8000", G1: "Escape", G2: "1", G3: "2", G4: "3", G5: "4", G6: "5", G7: "6", G8: "Tab", G9: "Q",
          G10: "W", G11: "E", G12: "R", G13: "T", G14: "M", G15: "LeftShift", G16: "A", G17: "S", G18: "D", G19: "F",
          G20: "LeftCtrl", G21: "C", G22: "Space", G23: "V", G24: "LeftAlt", G25: { mouse: "Middle" },
          STICK_UP: "W", STICK_DOWN: "S", STICK_LEFT: "A" },
        M2: { color: "#FFFFFF", G1: "LeftCtrl+C", G2: "LeftCtrl+V", G6: { macro: "Ctrl+S" }, G7: { wheel: 1 } },
        M3: { color: "#00FFFF", G2: { macro: "autoclick-toggle" } },
      },
      macros: {
        "Ctrl+S": { mode: "once", steps: ["down LeftCtrl", "down S", "wait 30", "up S", "up LeftCtrl", "wait 0"] },
        "autoclick-toggle": { mode: "toggle", steps: ["mouse_down Left", "wait 20", "mouse_up Left", "wait 80"] },
      },
    },
    notepad: { name: "Bloc-notes", rules: [{ exe: "notepad.exe" }], banks: { M1: { color: "#0000FF", G1: "F5" }, M2: {}, M3: {} }, macros: {} },
  };
  const q = new URLSearchParams(location.search);
  let settings = { autostart: false, start_minimized: true, close_to_tray: true, overlay_key: "G25", profiles_dir: "C:\\Users\\…\\g13d\\config\\profiles",
    language: q.get("lang"), keyboard_layout: q.get("kb") };
  const listeners = [];
  const emit = (payload) => listeners.filter((l) => l.name === "g13").forEach((l) => l.cb({ payload }));
  let headset = q.has("nohs") ? { present: false, connected: false } : { present: true, connected: !q.has("hsoff"), name: "PRO X Wireless",
    battery: 70, voltage_mv: 3958, charging: true, sidetone: 0, sleep_minutes: 10,
    volume: 96, muted: false, eq_installed: q.has("eq"),
    eq_profiles: [{ name: "", apps: [], gains: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }, { name: "Jeux FPS", apps: ["cs2.exe"], gains: [-4, -3, -2, -1, 0, 2, 4, 5, 3, 0] }],
    eq_active: 0, eq_default: 0, eq_pinned: null, audio_device: "Casque (2- Logitech PRO X Wireless Gaming Headset)",
    equalizer: { freqs: [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000], gains: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0], min_db: -12, max_db: 12 } };
  let mouse = q.has("nomouse") ? { connected: false } : {
    connected: true, name: "G502 X PLUS", battery: 93, charging: true, profile: 2, current_dpi: 1200,
    dpi_min: 100, dpi_max: 25600, dpi_step: 50, ghub_running: q.has("ghub"), message: null, error: false,
    settings: { dpi: [800, 1200, 1600, 2400, 3200], default_slot: 2, shift_slot: 0, report_rate_hz: 500, lighting: { effect: "signature" },
      buttons: [{ mouse: "Left" }, { mouse: "Right" }, { mouse: "Middle" }, { mouse: "Back" }, { special: "gshift" }, { mouse: "Forward" },
        { special: "wheel_left" }, { special: "wheel_right" }, { special: "profile_cycle" }, { special: "dpi_up" }, { special: "dpi_down" },
        null, null, null, null, null],
      gshift: [{ mouse: "Left" }, { mouse: "Right" }, "LeftCtrl+T", { mouse: "Back" }, null, { mouse: "Forward" }, "LeftCtrl+LeftShift+Tab",
        "LeftCtrl+Tab", "LeftCtrl+0", "LeftCtrl+Z", "LeftCtrl+X", null, null, null, null, null] },
  };
  const commands = {
    get_status: () => ({ connected: !location.hash.includes("offline"), bank: "M1", active_profile: "demo", recording: false, display_on: true, dry_run: true, error: null }),
    list_profiles: () => Object.entries(profiles).map(([id, p], i) => ({ id, number: i + 1, name: p.name, default: !!p.default, apps: p.rules.map((r) => r.exe || r.title), error: null })),
    set_preview: () => {},
    set_pinned: () => {},
    record_start: () => console.log("[mock] record"),
    record_stop: () => ["down LeftCtrl", "wait 82", "down C", "wait 64", "up C", "wait 40", "up LeftCtrl", "wait 300", "mouse_down Right", "wait 120", "mouse_up Right"],
    detected_apps: () => ({
      running: [{ exe: "Notepad.exe", title: "Sans titre – Bloc-notes" }, { exe: "zen.exe", title: "Twitch — Zen Browser" }],
      recent: [{ exe: "wardogs.exe", title: "WARDOGS" }],
    }),
    assign_game: ({ id, exe }) => { profiles[id].rules.push({ exe }); },
    load_profile: ({ id }) => structuredClone(profiles[id]),
    save_profile: ({ id, profile }) => {
      if (profile.default) for (const p of Object.values(profiles)) p.default = false;
      profiles[id] = structuredClone(profile);
      console.log("[mock] saved", id, profile);
    },
    create_profile: ({ name, from }) => {
      const id = name.toLowerCase().replace(/[^a-z0-9]+/g, "-");
      profiles[id] = from ? { ...structuredClone(profiles[from]), name, default: false, rules: [] } : { name, rules: [], banks: { M1: {}, M2: {}, M3: {} }, macros: {} };
      return id;
    },
    delete_profile: ({ id }) => { delete profiles[id]; },
    get_settings: () => settings,
    set_settings: (a) => { settings = { ...settings, autostart: a.autostart, start_minimized: a.startMinimized, close_to_tray: a.closeToTray, overlay_key: a.overlayKey, overlay_position: a.overlayPosition, auto_backup: a.autoBackup, auto_backup_keep: a.autoBackupKeep }; },
    backup_now: () => (settings.last_backup = "2026-10-17 15:30"),
    close_overlay: () => {},
    open_profiles_dir: () => {},
    app_version: () => "0.18.0",
    open_link: ({ which }) => { console.log("open_link", which); },
    key_names: () => { throw "not available in the mock"; },
    gamepad_status: () => null,
    set_language: ({ language, keyboardLayout }) => {
      const u = new URL(location.href);
      u.searchParams.set("lang", language || ""); u.searchParams.set("kb", keyboardLayout || "");
      location.href = u.toString();
    },
    export_config: () => "C:\\Users\\…\\G Nexus.g13backup",
    import_config: () => 2,
    reset_config: () => {},
    open_backups_dir: () => {},
    ui_language: () => {},
    get_headset: () => structuredClone(headset),
    set_headset_sidetone: ({ level }) => { headset = { ...headset, sidetone: level }; emit({ type: "headset", status: structuredClone(headset) }); },
    save_eq_profile: ({ index, profile }) => {
      const list = headset.eq_profiles.slice();
      let i = index;
      if (i == null) { list.push(profile); i = list.length - 1; } else list[i] = profile;
      headset = { ...headset, eq_profiles: list, eq_active: i, equalizer: { ...headset.equalizer, gains: profile.gains } };
      emit({ type: "headset", status: structuredClone(headset) });
    },
    delete_eq_profile: ({ index }) => { const list = headset.eq_profiles.filter((_, i) => i !== index); headset = { ...headset, eq_profiles: list, eq_active: headset.eq_default }; emit({ type: "headset", status: structuredClone(headset) }); },
    use_eq_profile: ({ index }) => { headset = { ...headset, eq_active: index }; emit({ type: "headset", status: structuredClone(headset) }); },
    set_eq_default: ({ index }) => { headset = { ...headset, eq_default: index }; emit({ type: "headset", status: structuredClone(headset) }); },
    pin_eq_profile: ({ index }) => { headset = { ...headset, eq_pinned: index, eq_active: index ?? headset.eq_default }; emit({ type: "headset", status: structuredClone(headset) }); },
    audio_eq_install: () => { headset = { ...headset, eq_installed: true }; emit({ type: "headset", status: structuredClone(headset) }); },
    audio_eq_uninstall: () => { headset = { ...headset, eq_installed: false }; emit({ type: "headset", status: structuredClone(headset) }); },
    set_headset_volume: ({ volume }) => { headset = { ...headset, volume }; emit({ type: "headset", status: structuredClone(headset) }); },
    set_headset_mute: ({ mute }) => { headset = { ...headset, muted: mute }; emit({ type: "headset", status: structuredClone(headset) }); },
    set_headset_sleep: ({ minutes }) => { headset = { ...headset, sleep_minutes: minutes }; emit({ type: "headset", status: structuredClone(headset) }); },
    set_headset_eq: ({ gains }) => { headset = { ...headset, equalizer: { ...headset.equalizer, gains } }; emit({ type: "headset", status: structuredClone(headset) }); },
    get_mouse: () => structuredClone(mouse),
    preview_mouse: (a) => console.log("[mock] preview", a),
    export_mouse_profile: () => "C:\\Users\\…\\G502 X PLUS.g13mouse",
    import_mouse_profile: () => null,
    select_mouse_profile: ({ sector }) => { mouse = { ...mouse, profile: sector, settings: structuredClone(mouse.profiles.find((p) => p.sector === sector).settings) }; emit({ type: "mouse", status: structuredClone(mouse) }); },
    enable_mouse_profile: ({ sector, enabled }) => { mouse.profiles.find((p) => p.sector === sector).enabled = enabled; emit({ type: "mouse", status: structuredClone(mouse) }); },
    revert_mouse: () => console.log("[mock] revert"),
    set_mouse_soft_default: ({ index }) => { mouse = { ...mouse, soft_default: index }; emit({ type: "mouse", status: structuredClone(mouse) }); },
    pin_mouse_soft: ({ index }) => { mouse = { ...mouse, soft_pinned: index, soft_active: index ?? mouse.soft_default ?? 0 }; emit({ type: "mouse", status: structuredClone(mouse) }); },
    macro_library: () => ({ "reload-fast": { mode: "once", steps: ["down R", "wait 20", "up R"] }, "spam-clic": { mode: "repeat_while_held", steps: ["mouse_down Left", "wait 30", "mouse_up Left", "wait 60"] } }),
    save_library_macro: (a) => console.log("[mock] library +", a.name),
    delete_library_macro: (a) => console.log("[mock] library -", a.name),
    set_mouse_host: ({ on }) => {
      if (on && !mouse.soft_profiles.length) mouse.soft_profiles = [{ name: "", apps: [], settings: structuredClone(mouse.settings) }];
      mouse = { ...mouse, host: on, soft_active: on ? 0 : null, message: on ? "Temps réel : G Nexus pilote la souris" : "Profils intégrés de la souris réactivés", error: false };
      if (on) mouse.settings = structuredClone(mouse.soft_profiles[0].settings);
      emit({ type: "mouse", status: structuredClone(mouse) });
    },
    save_mouse_soft: ({ index, profile }) => {
      if (index == null) mouse.soft_profiles.push(profile); else mouse.soft_profiles[index] = profile;
      mouse = { ...mouse, message: "Profil enregistré", error: false };
      emit({ type: "mouse", status: structuredClone(mouse) });
    },
    delete_mouse_soft: ({ index }) => {
      mouse.soft_profiles.splice(index, 1);
      emit({ type: "mouse", status: structuredClone(mouse) });
    },
    apply_mouse: ({ settings }) => {
      setTimeout(() => {
        mouse = { ...mouse, settings: structuredClone(settings), message: "Enregistré dans la souris (profil 2)", error: false };
        emit({ type: "mouse", status: structuredClone(mouse) });
      }, 600);
    },
  };
  mouse.host = q.has("host");
  mouse.soft_profiles = mouse.connected ? [{ name: "", apps: [], settings: structuredClone(mouse.settings) }, { name: "", apps: ["eldenring.exe"], settings: { ...structuredClone(mouse.settings), dpi: [400, 800] , default_slot: 0, shift_slot: 0 } }] : [];
  mouse.soft_active = mouse.host ? 0 : null;
  if (mouse.connected) mouse.profiles = [1, 2, 3, 4, 5].map((n) => ({ sector: n, enabled: n <= 2, settings: { ...structuredClone(mouse.settings), report_rate_hz: n === 2 ? 500 : 1000 } }));
  window.__TAURI__ = {
    core: { invoke: async (cmd, args = {}) => { if (!commands[cmd]) throw "unknown command " + cmd; return commands[cmd](args); } },
    event: { listen: async (name, cb) => { listeners.push({ name, cb }); return () => {}; } },
  };
  // simulate a key press from the console: g13mock.press("G5")
  window.g13mock = {
    press(key, ms = 300) {
      emit({ type: "key", key, pressed: true });
      setTimeout(() => emit({ type: "key", key, pressed: false }), ms);
    },
  };
})();
