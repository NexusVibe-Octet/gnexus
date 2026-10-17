// Blueprint drawing of the G502 X PLUS (same style as the G13), traced from
// the user's Gemini blueprint: a top view (default) and the left profile, one at a
// time, switched with the toggle under the drawing. Each button is an SVG
// group .key[data-btn] (index in the profile's button table); clicking it
// calls onButton(index, id).
//
// Profile slots of the G502 X PLUS (from its default functions):
//   0 G1 left click · 1 G2 right click · 2 G3 wheel click · 3 G4 back (thumb, rear)
//   4 G6 sniper (front of the thumb rest) · 5 G5 forward (thumb, front) · 6 / 7 wheel
//   tilt left / right · 8 G9 (behind the wheel) · 9 G8 / 10 G7 (left of the left click)
"use strict";

// Geometry: the user's Gemini blueprint (its own coordinates). Top view: body x 104–446,
// y 226–785, front at the top. Left profile: body x 557–1025, y 268–442, front on the left.
const MOUSE_BUTTONS = [
  // { slot, id, view, d: shape, at: label inside [x, y] | call: label outside, kid: where the
  // button name goes when not above the label, max: label length }
  // Small buttons are labelled outside (blueprint callouts): the leader runs from the
  // button (`from`) to the label (`to`) and is drawn under the buttons.
  { slot: 0, id: "G1", view: "top", at: [226, 430], max: 8,
    d: "M271 239 Q234 249 203 262 L188 372 L175 533 L271 495 Z" },
  { slot: 1, id: "G2", view: "top", at: [377, 392], max: 8,
    d: "M329 233 Q370 248 398 268 Q420 340 424 449 L332 480 Z" },
  { slot: 2, id: "G3", view: "top", call: { from: [319, 330], to: [482, 312], anchor: "start" }, max: 18,
    d: "M291 317 h18 a10 10 0 0 1 10 10 v72 a10 10 0 0 1 -10 10 h-18 a10 10 0 0 1 -10 -10 v-72 a10 10 0 0 1 10 -10 Z" },
  { slot: 6, id: "◂", view: "top", call: { from: [259, 363], to: [118, 384], anchor: "end" }, max: 18,
    d: "M271 351 L257 363 L271 375 Z" },
  { slot: 7, id: "▸", view: "top", call: { from: [341, 363], to: [482, 384], anchor: "start" }, max: 18,
    d: "M329 351 L343 363 L329 375 Z" },
  { slot: 8, id: "G9", view: "top", call: { from: [309, 486], to: [482, 486], anchor: "start" }, max: 18,
    d: "M295 472 h10 a4 4 0 0 1 4 4 v20 a4 4 0 0 1 -4 4 h-10 a4 4 0 0 1 -4 -4 v-20 a4 4 0 0 1 4 -4 Z" },
  { slot: 9, id: "G8", view: "top", call: { from: [166, 300], to: [118, 286], anchor: "end" }, max: 18,
    d: "M203 262 L182 267 Q168 310 156 368 L188 372 Z" },
  { slot: 10, id: "G7", view: "top", call: { from: [152, 452], to: [118, 482], anchor: "end" }, max: 18,
    d: "M188 375 L155 372 Q145 430 144 468 L175 533 Z" },
  { slot: 5, id: "G5", view: "side", at: [772, 342], kid: [772, 372], max: 7,
    d: "M736 338 L809 326 L809 350 L736 358 Z" },
  { slot: 3, id: "G4", view: "side", at: [847, 336], kid: [847, 364], max: 7,
    d: "M812 325 L885 322 L881 344 L812 350 Z" },
  { slot: 4, id: "G6", view: "side", call: { from: [722, 400], to: [650, 486], anchor: "end" }, max: 18,
    d: "M702 368 L744 366 L768 408 L722 412 Z" },
];

const MouseDevice = {
  el: null,
  views: {},
  view: "top",

  build(host, onButton) {
    this.el = host;
    const ns = "http://www.w3.org/2000/svg";
    const grip = (id) => `<pattern id="${id}" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(35)">
      <path d="M0 0 V6" class="hatch"/></pattern>`;
    const wheel = (id) => `<pattern id="${id}" width="4" height="4" patternUnits="userSpaceOnUse"><path d="M0 2 H4" class="hatch"/></pattern>`;
    // each view has its own gradient: a hidden SVG's definitions are not usable by the other
    const stripGrad = (id) => `<linearGradient id="${id}" x1="0" x2="1"><stop offset="0" stop-color="var(--strip-a)"/><stop offset="1" stop-color="var(--strip-b)"/></linearGradient>`;
    const logo = "M25 4 A20 20 0 1 0 25 44 A20 20 0 0 0 43 29 L43 22 L25 22 L25 30 L34 30 A11 11 0 1 1 25 13 Z";

    const top = document.createElementNS(ns, "svg");
    top.setAttribute("viewBox", "-30 214 640 614");
    top.innerHTML = `
      <defs>${stripGrad("strip-top")}${grip("grip-top")}${wheel("wheel-top")}</defs>
      <path class="shell" d="M271 236 L322 226 L329 233 Q372 248 400 269 Q424 335 436 418 Q444 480 438 552 Q446 615 443 662
        Q436 710 412 745 Q375 785 295 785 Q222 784 190 735 Q172 695 158 656 Q135 630 106 604 Q104 545 123 477 L141 462
        Q142 405 153 352 Q166 298 182 267 Q224 248 271 236 Z"/>
      <!-- textured thumb wing -->
      <path class="grip" fill="url(#grip-top)" d="M141 462 L123 477 Q104 545 106 604 Q135 630 158 656 Q172 685 182 700 Q164 625 160 540 Z"/>
      <!-- centre column (wheel, mode button, G9) and the click seams -->
      <path class="seam" d="M271 236 L271 515 Q271 526 282 526 L322 515 Q332 512 332 500 L329 233"/>
      <path class="seam" d="M175 533 L271 495 M332 480 L424 449"/>
      <rect class="led" x="287" y="281" width="24" height="6" rx="3" transform="rotate(-8 299 284)"/>
      <rect class="deco" x="287" y="422" width="26" height="82" rx="6"/>
      <rect class="deco-line" x="291" y="426" width="18" height="32" rx="3"/>
      <circle class="deco-line" cx="300" cy="442" r="5"/>
      <path class="badge" transform="translate(204 540) scale(1.15)" d="${logo}"/>
      <!-- LIGHTSYNC strip, the Y on the palm rest -->
      <path class="strip-fill" fill="url(#strip-top)" d="M164 536 Q180 612 200 638 Q255 674 368 666 Q384 667 394 695 L402 746 L412 742
        L398 672 Q396 652 406 624 Q422 570 430 510 Q416 582 394 638 Q385 656 365 656 Q258 660 206 626 Q184 602 164 536 Z"/>
      <text class="vcap" x="275" y="818">${t("VUE DU DESSUS")}</text>`;

    const side = document.createElementNS(ns, "svg");
    side.setAttribute("viewBox", "470 236 640 290");
    side.innerHTML = `
      <defs>${stripGrad("strip-side")}${grip("grip-side")}${wheel("wheel-side")}</defs>
      <path class="shell" d="M557 372 Q595 340 638 318 Q660 284 705 288 L732 276 L754 274 L760 268 L781 268 Q855 266 915 292
        Q972 315 1008 368 Q1026 398 1025 415 L1005 424 Q962 438 925 442 L718 442 L696 435 L648 434 Q604 405 557 372 Z"/>
      <!-- textured side grip -->
      <path class="grip" fill="url(#grip-side)" d="M702 365 L885 338 Q935 365 958 412 L915 436 L725 436 Q710 400 702 365 Z"/>
      <!-- wheel peeking out, shell seams -->
      <path class="deco" fill="url(#wheel-side)" d="M638 318 Q662 278 704 290 Z"/>
      <path class="seam" d="M558 372 Q665 325 765 305 L915 292 M568 380 Q675 342 745 330"/>
      <path class="strip-fill" fill="url(#strip-side)" d="M755 314 L908 304 L918 293 Q914 328 892 346 L878 342 Q892 325 894 314 L755 318 Z"/>
      <text class="vcap" x="790" y="516">${t("PROFIL GAUCHE")}</text>`;

    const switcher = document.createElement("div");
    switcher.className = "seg mdev-switch";
    switcher.innerHTML = `<button data-view="top" class="on">${t("VUE DU DESSUS")}</button><button data-view="side">${t("PROFIL GAUCHE")}</button>`;
    switcher.addEventListener("click", (e) => {
      const b = e.target.closest("button[data-view]");
      if (b) this.show(b.dataset.view);
    });

    const stage = document.createElement("div");
    stage.className = "mdev-stage";
    stage.append(top, side);
    host.append(stage, switcher);
    this.views = { top, side };

    // leaders first: the buttons are drawn over them
    for (const b of MOUSE_BUTTONS.filter((b) => b.call)) {
      const { from, to, anchor } = b.call;
      const end = [to[0] + (anchor === "end" ? 6 : -6), to[1] - 4];
      const svg = this.views[b.view];
      const p = document.createElementNS(ns, "path");
      p.setAttribute("class", "leader");
      p.setAttribute("d", `M${from[0]} ${from[1]} L${end[0]} ${end[1]}`);
      svg.appendChild(p);
      const dot = document.createElementNS(ns, "circle");
      dot.setAttribute("class", "leader-dot");
      Object.entries({ cx: from[0], cy: from[1], r: 2.2 }).forEach(([k, v]) => dot.setAttribute(k, v));
      svg.appendChild(dot);
    }
    for (const b of MOUSE_BUTTONS) {
      const g = document.createElementNS(ns, "g");
      g.setAttribute("class", "key" + (b.call ? " callout" : ""));
      g.dataset.btn = b.slot;
      const [x, y] = b.at || b.call.to;
      const anchor = b.call ? b.call.anchor : "middle";
      g.innerHTML = `<title></title><path class="kshape" d="${b.d}"/>` +
        (b.slot === 2 ? `<path class="ridges" d="${Array.from({ length: 9 }, (_, i) => `M286 ${327 + i * 9} h28`).join(" ")}"/>` : "") +
        (b.slot === 4 ? `<circle class="ridges" cx="733" cy="390" r="6"/><path class="ridges" d="M733 381 v18 M724 390 h18"/>` : "") +
        `<text class="kid" x="${b.kid ? b.kid[0] : x}" y="${b.kid ? b.kid[1] : y - 14}" text-anchor="${anchor}">${b.id}</text>` +
        `<text class="klabel" x="${x}" y="${y + 4}" text-anchor="${anchor}"></text>`;
      g.addEventListener("click", () => onButton(b.slot, b.id));
      this.views[b.view].appendChild(g);
    }
    this.show("top");
  },

  /** Show the top view or the left profile. */
  show(view) {
    this.view = view;
    for (const [k, svg] of Object.entries(this.views)) svg.classList.toggle("hidden", k !== view);
    for (const b of this.el.querySelectorAll(".mdev-switch button")) b.classList.toggle("on", b.dataset.view === view);
  },

  /** `labels[slot]` = text shown on the button ("" = nothing), `titles[slot]` = full text on hover. */
  setLabels(labels, titles, disabled = new Set()) {
    for (const { slot, id, max, call } of MOUSE_BUTTONS) {
      const g = this.el.querySelector(`.key[data-btn="${slot}"]`);
      const full = labels[slot] || "";
      const l = g.querySelector(".klabel");
      const chars = [...full];
      l.textContent = chars.length > max ? chars.slice(0, max - 1).join("") + "…" : full || "—";
      l.classList.toggle("long", !call && chars.length > 6);
      g.classList.toggle("empty", !full);
      g.classList.toggle("disabled", disabled.has(slot));
      g.querySelector("title").textContent = `${id === "◂" ? t("Molette inclinée à gauche") : id === "▸" ? t("Molette inclinée à droite") : id} : ${titles[slot] || "—"}`;
    }
  },

  /** Real time: light up the buttons held on the mouse (to check which is which). */
  setPressed(slots) {
    if (!this.el) return;
    for (const g of this.el.querySelectorAll(".key")) g.classList.toggle("pressed", slots.includes(+g.dataset.btn));
  },

  setSelected(slot) {
    if (!this.el) return;
    for (const g of this.el.querySelectorAll(".key.selected")) g.classList.remove("selected");
    if (slot != null) this.el.querySelector(`.key[data-btn="${slot}"]`)?.classList.add("selected");
  },
};
