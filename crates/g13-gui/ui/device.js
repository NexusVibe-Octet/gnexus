// Drawing of the Logitech G13 (design space 920 × 930, scaled to fit).
//
// Geometry follows the real device (photo):
//  - body height ≈ 1.45 × its width; the wrist rest takes the lower third;
//  - key well: rows 1–2 almost straight-sided, rows 3–4 slanted inward
//    (hourglass); edge keys are trapezoids following the frame;
//  - row 1 G1–G7 and row 2 G8–G14 on the same 7 columns;
//  - row 3: G15 (wide, slanted) · G16 G17 G18 under G10 G11 G12 · G19 (wide);
//  - row 4: G20 (wide, slanted) · G21 under G17 · G22 (wide);
//  - the thumb module is drawn as a separate card on the right: stick with
//    its 4 directions (bindable in keys mode) around the click G25, G23, G24
//    and the dead zone.
"use strict";

const DEVICE_W = 920;
const DEVICE_H = 930;
const BODY_W = 760; // the G13 body is symmetric around x = 380
const SVGNS = "http://www.w3.org/2000/svg";

const f1 = (n) => Math.round(n * 10) / 10;

/** Path of a polygon with rounded corners. */
function roundPoly(pts, r) {
  let d = "";
  const n = pts.length;
  for (let i = 0; i < n; i++) {
    const p0 = pts[(i - 1 + n) % n], p1 = pts[i], p2 = pts[(i + 1) % n];
    const v1 = [p0[0] - p1[0], p0[1] - p1[1]], v2 = [p2[0] - p1[0], p2[1] - p1[1]];
    const l1 = Math.hypot(...v1), l2 = Math.hypot(...v2);
    const rr = Math.min(r, l1 / 2, l2 / 2);
    const a = [f1(p1[0] + (v1[0] / l1) * rr), f1(p1[1] + (v1[1] / l1) * rr)];
    const b = [f1(p1[0] + (v2[0] / l2) * rr), f1(p1[1] + (v2[1] / l2) * rr)];
    d += `${i ? "L" : "M"}${a[0]} ${a[1]} Q${f1(p1[0])} ${f1(p1[1])} ${b[0]} ${b[1]} `;
  }
  return d + "Z";
}

/** Annular sector (angles in degrees, 0 = right, 90 = down). */
function sector(cx, cy, r0, r1, a0, a1) {
  const p = (r, a) => [f1(cx + r * Math.cos((a * Math.PI) / 180)), f1(cy + r * Math.sin((a * Math.PI) / 180))];
  const [x0, y0] = p(r1, a0), [x1, y1] = p(r1, a1), [x2, y2] = p(r0, a1), [x3, y3] = p(r0, a0);
  return `M${x0} ${y0} A${r1} ${r1} 0 0 1 ${x1} ${y1} L${x2} ${y2} A${r0} ${r0} 0 0 0 ${x3} ${y3} Z`;
}

// ---- key geometry ------------------------------------------------------------------

const ROWS = [[248, 306], [314, 372], [380, 438], [446, 504]];
const L12 = (y) => 120 + 0.07 * (y - 248); // frame side, rows 1–2
const L3 = (y) => 165 + 0.55 * (y - 380); //  row 3 edge
const L4 = (y) => 222 + 0.55 * (y - 446); //  row 4 edge
const mirror = (fn) => (y) => BODY_W - fn(y);
const colX = (i) => 380 + (i - 3) * 74; //   column centres (7 columns, pitch 74, keys 66 wide)

function rect(x0, x1, [y0, y1]) {
  return [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
}
function leftKey(edge, x1, [y0, y1]) {
  return [[edge(y0), y0], [x1, y0], [x1, y1], [edge(y1), y1]];
}
function rightKey(x0, edge, [y0, y1]) {
  return [[x0, y0], [edge(y0), y0], [edge(y1), y1], [x0, y1]];
}

const KEY_POLYS = (() => {
  const k = {};
  for (const [row, first] of [[0, 1], [1, 8]]) {
    const ys = ROWS[row];
    k["G" + first] = leftKey(L12, colX(0) + 33, ys);
    for (let i = 1; i <= 5; i++) k["G" + (first + i)] = rect(colX(i) - 33, colX(i) + 33, ys);
    k["G" + (first + 6)] = rightKey(colX(6) - 33, mirror(L12), ys);
  }
  k.G15 = leftKey(L3, colX(2) - 41, ROWS[2]);
  k.G16 = rect(colX(2) - 33, colX(2) + 33, ROWS[2]);
  k.G17 = rect(colX(3) - 33, colX(3) + 33, ROWS[2]);
  k.G18 = rect(colX(4) - 33, colX(4) + 33, ROWS[2]);
  k.G19 = rightKey(colX(4) + 41, mirror(L3), ROWS[2]);
  k.G20 = leftKey(L4, colX(3) - 41, ROWS[3]);
  k.G21 = rect(colX(3) - 33, colX(3) + 33, ROWS[3]);
  k.G22 = rightKey(colX(3) + 41, mirror(L4), ROWS[3]);
  return k;
})();

// thumb module: separate card on the right of the body
const CARD = { x: 716, y: 432, w: 180, h: 358, r: 22 };
const STICK = { cx: 806, cy: 548, knob: 24, r0: 30, r1: 54 };
const STICK_DIRS = [
  ["STICK_UP", -90, "↑"], ["STICK_RIGHT", 0, "→"], ["STICK_DOWN", 90, "↓"], ["STICK_LEFT", 180, "←"],
];
const THUMB_KEYS = {
  G23: rect(728, 798, [628, 670]),
  G24: rect(814, 884, [628, 670]),
};

// body: ≈ 612 wide × 884 high, wrist rest flaring out below the waist (y ≈ 580)
const SHELL = `M210 24 L550 24 Q588 24 600 58 L640 168 Q646 186 662 198 L676 214 Q688 228 686 248 L677 362
  Q674 382 656 398 L572 520 Q556 548 558 600 L648 836 Q666 896 610 908 L150 908 Q94 896 112 836 L202 600
  Q204 548 188 520 L104 398 Q86 382 83 362 L74 248 Q72 228 84 214 L98 198 Q114 186 120 168 L160 58
  Q172 24 210 24 Z`;
const BEZEL = "M107 236 H653 L643 368 Q640 382 610 386 L518 516 H242 L150 386 Q120 382 117 368 Z";
const PALM = "M222 652 L538 652 Q554 652 560 666 L630 846 Q636 884 600 886 L160 886 Q124 884 130 846 L200 666 Q206 652 222 652 Z";
const NOTCH = "M330 908 L344 870 Q348 862 358 862 L402 862 Q412 862 416 870 L430 908";

function el(tag, attrs = {}, parent) {
  const e = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  if (parent) parent.appendChild(e);
  return e;
}

function centroid(pts) {
  return [pts.reduce((s, p) => s + p[0], 0) / pts.length, pts.reduce((s, p) => s + p[1], 0) / pts.length];
}

const Device = {
  el: null,
  svg: null,

  /**
   * Build into `host`. `onKey(id)` for bindable inputs, `onBank(bank)` for M1–M3,
   * `onMove(from, to)` when a key is dragged onto another one.
   */
  build(host, { onKey, onBank, onMove }) {
    host.innerHTML = "";
    this.el = host;
    const svg = el("svg", { class: "body", viewBox: `0 0 ${DEVICE_W} ${DEVICE_H}` }, host);
    this.svg = svg;
    svg.innerHTML = `
      <path class="shell" d="${SHELL}"/>
      <path class="inner" d="M196 42 L564 42 Q578 42 582 56 L610 180 L150 180 L178 56 Q182 42 196 42 Z"/>
      <path class="bezel" d="${BEZEL}"/>
      <path class="inner" d="${PALM}"/>
      <path class="inner" d="${NOTCH}"/>
      <text class="badge" x="380" y="604" text-anchor="middle">G13</text>
      <path class="link" d="M566 ${STICK.cy} H${CARD.x}"/>
      <circle class="link-dot" cx="566" cy="${STICK.cy}" r="3"/>
      <rect class="card" x="${CARD.x}" y="${CARD.y}" width="${CARD.w}" height="${CARD.h}" rx="${CARD.r}"/>
      <text class="card-cap" x="${STICK.cx}" y="${CARD.y + 30}" text-anchor="middle">STICK</text>
      <circle class="inner" cx="${STICK.cx}" cy="${STICK.cy}" r="${STICK.r1 + 6}"/>
      <path class="sep" d="M${CARD.x + 16} 610 H${CARD.x + CARD.w - 16}"/>`;

    const addKey = (id, pts, radius = 10) => {
      const g = el("g", { class: "key", "data-key": id }, svg);
      el("path", { class: "kshape", d: roundPoly(pts, radius) }, g);
      const [cx, cy] = centroid(pts);
      el("path", { class: "kx", d: `M${f1(cx - 9)} ${f1(cy - 5)} l18 18 M${f1(cx + 9)} ${f1(cy - 5)} l-18 18` }, g);
      // top-left corner (works for slanted and rotated keys)
      const top = pts.reduce((m, p) => (p[0] + p[1] < m[0] + m[1] ? p : m));
      const kid = el("text", { class: "kid", x: f1(top[0] + 8), y: f1(top[1] + 13) }, g);
      kid.textContent = id;
      el("text", { class: "klabel", x: f1(cx), y: f1(cy + 5) }, g);
      g.addEventListener("click", () => onKey(id));
      return g;
    };

    for (const [id, pts] of Object.entries(KEY_POLYS)) addKey(id, pts);
    for (const [id, pts] of Object.entries(THUMB_KEYS)) {
      const g = addKey(id, pts, 9);
      g.classList.add("thumb");
    }

    // stick: 4 direction pads + knob (G25)
    for (const [id, a, arrow] of STICK_DIRS) {
      const g = el("g", { class: "key pad", "data-key": id, "data-arrow": arrow }, svg);
      el("path", { class: "kshape", d: sector(STICK.cx, STICK.cy, STICK.r0, STICK.r1, a - 38, a + 38) }, g);
      const rm = (STICK.r0 + STICK.r1) / 2;
      const tx = STICK.cx + rm * Math.cos((a * Math.PI) / 180), ty = STICK.cy + rm * Math.sin((a * Math.PI) / 180);
      el("text", { class: "klabel", x: f1(tx), y: f1(ty + 4) }, g);
      g.addEventListener("click", () => onKey(id));
    }
    const knob = el("g", { class: "key knob", "data-key": "G25" }, svg);
    el("circle", { class: "kshape", cx: STICK.cx, cy: STICK.cy, r: STICK.knob }, knob);
    const kt = el("text", { class: "kid", x: STICK.cx, y: STICK.cy - 9, "text-anchor": "middle" }, knob);
    kt.textContent = "G25";
    el("text", { class: "klabel", x: STICK.cx, y: STICK.cy + 9 }, knob);
    knob.addEventListener("click", () => onKey("G25"));

    if (onMove) this.dragKeys(svg, onMove);

    // HTML overlays: LCD, its buttons, M strip, colour + dead-zone controls
    const add = (cls, id, x, y, w, h, html = "") => {
      const d = document.createElement("div");
      d.className = cls;
      if (id) d.dataset.key = id;
      Object.assign(d.style, { left: x + "px", top: y + "px", width: w + "px", height: h + "px" });
      d.innerHTML = html;
      host.appendChild(d);
      return d;
    };
    add("lcd", null, 262, 60, 236, 66,
      '<canvas id="lcd-canvas" width="160" height="43"></canvas>' +
      '<div class="lcd-text"><div class="lcd-line big" id="lcd-1"></div><div class="lcd-line" id="lcd-2"></div><div class="lcd-line" id="lcd-3"></div></div>');
    add("hw round", "BD", 212, 140, 28, 28).title = t("BD — applet suivant sur l'écran (Profil, Profils, Média, Horloge, Système)");
    ["L1", "L2", "L3", "L4"].forEach((id, i) => {
      add("hw lbtn", id, 270 + i * 58, 148, 46, 13).title = t("{id} — touche de l'applet affiché (libellé en bas de l'écran)", { id });
    });
    add("hw round small deco", null, 516, 144, 20, 20,
      '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="3.5"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M5.6 18.4 7 17M17 7l1.4-1.4"/></svg>')
      .title = t("Rétroéclairage matériel (géré par le G13)");
    ["M1", "M2", "M3", "MR"].forEach((id, i) => {
      const m = add("mkey", id, 200 + i * 92, 194, 84, 30, `<span>${id}</span>`);
      if (id !== "MR") {
        m.title = t("Afficher le menu {m}", { m: id });
        m.addEventListener("click", () => onBank(id));
      } else {
        m.title = t("MR — enregistrement de macro (capture : bientôt)");
      }
    });
    add("color-ctl", null, 280, 662, 200, 150,
      `<button class="color-wheel" id="color-wheel" title="${t("Choisir la couleur")}"><i></i></button>
       <button class="chip-btn" id="color-btn">${t("COULEUR")}</button>
       <input type="range" id="bright" min="5" max="100" value="100">
       <div class="bright-val"><svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M5 19l1.5-1.5M17.5 6.5 19 5"/></svg><span id="bright-val">100%</span></div>`);
    add("pad-ctl", null, CARD.x + 14, 682, CARD.w - 28, 30,
      `<button data-target="left" title="${t("Le stick du G13 pilote le joystick gauche de la manette (déplacement)")}">${t("GAUCHE")}</button>` +
      `<button data-target="right" title="${t("Le stick du G13 pilote le joystick droit de la manette (caméra)")}">${t("DROIT")}</button>`);
    add("dz-ctl", null, CARD.x + 10, 696, CARD.w - 20, 70,
      `<div class="dz-val">${t("ZONE MORTE :")} <b id="dz-val">35 %</b></div><input type="range" id="deadzone" min="5" max="90" value="35">`);
  },

  /** Fit the 760×820 design space into the wrapper. */
  fit(wrap) {
    const s = Math.min(wrap.clientWidth / DEVICE_W, wrap.clientHeight / DEVICE_H);
    this.el.style.transform = `translate(-50%, -50%) scale(${Math.max(0.3, s)})`;
  },

  key(id) {
    return this.el.querySelector(`[data-key="${id}"]`);
  },

  setLabels(labels) {
    for (const k of this.el.querySelectorAll(".key")) {
      const id = k.dataset.key;
      const full = labels[id] || "";
      let text = full;
      const l = k.querySelector(".klabel");
      const pad = k.classList.contains("pad");
      // what fits on the key at the smallest font; the full name shows on hover
      const max = pad || k.classList.contains("thumb") || k.classList.contains("knob") ? 6 : 10;
      if ([...text].length > max) text = [...text].slice(0, max - 1).join("") + "…";
      let tip = k.querySelector("title");
      if (!tip) tip = k.insertBefore(document.createElementNS("http://www.w3.org/2000/svg", "title"), k.firstChild);
      tip.textContent = full ? `${id} : ${full}` : id;
      l.textContent = text || (pad ? k.dataset.arrow : "—");
      k.classList.toggle("empty", !text);
      l.classList.toggle("long", text.length > 5 || (k.classList.contains("thumb") && text.length > 2));
      l.classList.toggle("xlong", text.length > 9);
    }
  },

  setStickMode(mode, target = "left") {
    for (const p of this.el.querySelectorAll(".key.pad")) p.classList.toggle("disabled", mode !== "keys");
    for (const b of this.el.querySelectorAll(".pad-ctl button")) b.classList.toggle("on", b.dataset.target === target);
  },

  setBank(bank) {
    for (const m of this.el.querySelectorAll(".mkey")) m.classList.toggle("on", m.dataset.key === bank);
  },

  setRecording(on) {
    this.key("MR")?.classList.toggle("rec", on);
  },

  /**
   * Drag a key onto another one: past a few pixels the press becomes a drag (no click),
   * the key under the cursor lights up, and releasing on it calls `onMove(from, to)`.
   */
  dragKeys(svg, onMove) {
    let drag = null;
    const keyAt = (x, y) => document.elementFromPoint(x, y)?.closest?.("#device [data-key]");
    const end = () => {
      if (!drag) return;
      drag.ghost?.remove();
      drag.src.classList.remove("drag-src");
      drag.over?.classList.remove("drop-target");
      document.body.classList.remove("key-dragging");
      drag = null;
    };
    svg.addEventListener("pointerdown", (e) => {
      const src = e.button === 0 && e.target.closest("[data-key]");
      if (!src) return;
      drag = { src, id: src.dataset.key, x: e.clientX, y: e.clientY, on: false };
    });
    window.addEventListener("pointermove", (e) => {
      if (!drag) return;
      if (!drag.on) {
        if (Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 6) return;
        drag.on = true;
        drag.src.classList.add("drag-src");
        document.body.classList.add("key-dragging");
        drag.ghost = document.createElement("div");
        drag.ghost.className = "key-ghost";
        drag.ghost.textContent = drag.src.querySelector(".klabel")?.textContent || drag.id;
        document.body.append(drag.ghost);
      }
      drag.ghost.style.transform = `translate(${e.clientX + 14}px, ${e.clientY + 10}px)`;
      const over = keyAt(e.clientX, e.clientY);
      const target = over && over !== drag.src ? over : null;
      if (target !== drag.over) {
        drag.over?.classList.remove("drop-target");
        target?.classList.add("drop-target");
        drag.over = target;
      }
    });
    window.addEventListener("pointerup", () => {
      if (!drag) return;
      const { on, id, over } = drag;
      end();
      if (!on) return;
      // a drag is not a click: the key under the cursor must not open the picker
      // (the click, if any, follows this pointerup at once; never swallow a later one)
      const swallow = (c) => { c.stopPropagation(); c.preventDefault(); };
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);
      if (over) onMove(id, over.dataset.key);
    });
    window.addEventListener("pointercancel", end);
    window.addEventListener("blur", end);
  },

  setSelected(id) {
    for (const k of this.el.querySelectorAll(".key.selected")) k.classList.remove("selected");
    if (id) this.key(id)?.classList.add("selected");
  },

  setLive(id, pressed) {
    this.key(id)?.classList.toggle("live", pressed);
  },

  /** Show the real LCD image (960 bytes: 6 pages × 160 columns, bit 0 = top) in `color`. */
  setLcdFrame(frame, color = "#ffffff") {
    const canvas = this.el.querySelector("#lcd-canvas");
    if (!canvas || !frame || frame.length < 960) return;
    const ctx = canvas.getContext("2d");
    const img = ctx.createImageData(160, 43);
    const c = /^#?([0-9a-f]{6})$/i.exec(color);
    const n = c ? parseInt(c[1], 16) : 0xffffff;
    const [r, g, b] = [n >> 16, (n >> 8) & 255, n & 255];
    for (let y = 0; y < 43; y++) {
      for (let x = 0; x < 160; x++) {
        if (frame[(y >> 3) * 160 + x] & (1 << (y & 7))) {
          const i = (y * 160 + x) * 4;
          img.data[i] = r; img.data[i + 1] = g; img.data[i + 2] = b; img.data[i + 3] = 255;
        }
      }
    }
    ctx.putImageData(img, 0, 0);
    this.el.querySelector(".lcd").classList.add("live");
    this.lastFrame = frame;
  },

  setLcd(lines) {
    lines.forEach((t, i) => {
      const l = this.el.querySelector("#lcd-" + (i + 1));
      if (l) l.textContent = t;
    });
  },
};
