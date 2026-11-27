// Canvas player for the Replay tab. Owns the playback clock and all drawing;
// the Rust side owns the data, the controls and the entity cards.
window.BarasReplay = (() => {
  const TAU = Math.PI * 2;
  // Samples further apart than this are not a continuous move: hold, then ease in
  const HOLD_GAP_SECS = 1.5;
  const EASE_SECS = 0.5;
  // A jump this far (world units) is a teleport: snap
  const SNAP_DIST = 15;
  // A height change this large between samples is a lift or a drop: snap
  const SNAP_HEIGHT = 4;
  // An NPC stays drawn this long after its last log line
  const NPC_LINGER_SECS = 2;
  const TICK_MS = 100;
  const PICK_PX = 18;
  const GRID_STEPS = [1, 2, 5, 10, 20, 50, 100, 200, 500];

  // Last index with ts[i] <= t, or -1
  function indexAt(ts, t) {
    let lo = 0;
    let hi = ts.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (ts[mid] <= t) lo = mid + 1;
      else hi = mid;
    }
    return lo - 1;
  }

  function lerpAngle(a, b, f) {
    const d = ((((b - a) % 360) + 540) % 360) - 180;
    return a + d * f;
  }

  function isDead(e, t) {
    return e.dead.some(([from, to]) => t >= from && t < to);
  }

  // Which map entry a world position is on at a phase, or -1
  function mapIndexAt(maps, x, y, z, phase) {
    for (let i = 0; i < maps.length; i++) {
      const m = maps[i];
      const [x0, x1] = m.min_x < m.max_x ? [m.min_x, m.max_x] : [m.max_x, m.min_x];
      const [y0, y1] = m.min_y < m.max_y ? [m.min_y, m.max_y] : [m.max_y, m.min_y];
      if (x < x0 || x > x1 || y < y0 || y > y1) continue;
      if (m.min_z != null && z < m.min_z) continue;
      if (m.max_z != null && z > m.max_z) continue;
      if (m.phases && m.phases.length && !m.phases.includes(phase)) continue;
      return i;
    }
    return -1;
  }

  // Position, facing and presence of an entity at t; null when it is not there.
  // `mapOf(x, y, z)` says which map entry a position is on, when there are maps.
  function stateAt(e, t, duration, mapOf) {
    const tr = e.track;
    const n = tr.t.length;
    if (n === 0) return null;
    const npc = e.kind === "Npc";
    let i = indexAt(tr.t, t);
    if (i < 0) {
      if (npc) return null;
      i = 0;
    }
    if (npc && i === n - 1 && t > tr.t[i] + NPC_LINGER_SECS && tr.t[i] < duration - NPC_LINGER_SECS) {
      return null;
    }
    const dead = isDead(e, t);
    if (npc && dead) return null;
    const s = { x: tr.x[i], y: tr.y[i], z: tr.z[i], facing: tr.facing[i], dead };
    const j = i + 1;
    if (j >= n || t <= tr.t[i]) return s;
    const gap = tr.t[j] - tr.t[i];
    if (Math.hypot(tr.x[j] - s.x, tr.y[j] - s.y) > SNAP_DIST) return s;
    if (Math.abs(tr.z[j] - s.z) > SNAP_HEIGHT) return s;
    // Never glide from one map entry onto another
    if (mapOf && mapOf(s.x, s.y, s.z) !== mapOf(tr.x[j], tr.y[j], tr.z[j])) return s;
    // A long silence: stand still, then move over the last half second
    const start = gap > HOLD_GAP_SECS ? tr.t[j] - EASE_SECS : tr.t[i];
    if (t <= start) return s;
    const f = (t - start) / (tr.t[j] - start);
    s.x += (tr.x[j] - s.x) * f;
    s.y += (tr.y[j] - s.y) * f;
    s.z += (tr.z[j] - s.z) * f;
    s.facing = lerpAngle(s.facing, tr.facing[j], f);
    return s;
  }

  // Centre and span of the area the fight used, ignoring far outliers
  function fitBounds(entities) {
    const xs = [];
    const ys = [];
    for (const e of entities) {
      if (e.kind !== "Player") continue;
      for (let i = 0; i < e.track.t.length; i++) {
        xs.push(e.track.x[i]);
        ys.push(e.track.y[i]);
      }
    }
    if (!xs.length) return { cx: 0, cy: 0, w: 50, h: 50 };
    xs.sort((a, b) => a - b);
    ys.sort((a, b) => a - b);
    const lo = Math.floor(xs.length * 0.02);
    const hi = Math.ceil(xs.length * 0.98) - 1;
    const w = Math.max(xs[hi] - xs[lo], 20) * 1.3;
    const h = Math.max(ys[hi] - ys[lo], 20) * 1.3;
    return { cx: (xs[lo] + xs[hi]) / 2, cy: (ys[lo] + ys[hi]) / 2, w, h };
  }

  function create(canvas, data, meta) {
    const ctx = canvas.getContext("2d");
    const duration = data.duration_secs;
    const entities = data.entities;
    const icons = new Map();
    for (const [id, url] of Object.entries(meta.icons || {})) {
      const img = new Image();
      img.onload = requestDraw;
      img.src = url;
      icons.set(Number(id), img);
    }
    const numbers = meta.numbers || {};
    const fit = fitBounds(entities);

    let time = 0;
    let playing = false;
    let speed = 1;
    let scale = 1;
    let pan = { x: 0, y: 0 };
    let highlight = null;
    let raf = 0;
    let lastFrame = 0;
    let lastTick = 0;
    let tickCb = null;
    let pickCb = null;
    let mapCb = null;
    let destroyed = false;
    let colors = {};
    // Maps: entries, their images, phase spans, the pinned entry (null = follow the raid)
    let maps = [];
    let mapImages = [];
    let phases = [];
    let pinned = null;
    let shownMap = -1;
    // Screen positions of what was last drawn, for picking
    let drawn = [];

    function readColors() {
      const css = getComputedStyle(canvas);
      const v = (name, fallback) => css.getPropertyValue(name).trim() || fallback;
      colors = {
        bg: v("--bg-dark", "#1a1a1f"),
        grid: v("--border-light", "rgba(255,255,255,0.1)"),
        text: v("--text-secondary", "#ccc"),
        muted: v("--text-muted", "#888"),
        npc: v("--swtor-red", "#ff4a4a"),
        player: v("--swtor-blue", "#4a9eff"),
        gold: v("--swtor-gold", "#c9a227"),
      };
    }

    function resize() {
      const dpr = window.devicePixelRatio || 1;
      const { clientWidth: w, clientHeight: h } = canvas;
      if (!w || !h) return;
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
      readColors();
      requestDraw();
    }

    function phaseAt(t) {
      for (const p of phases) if (t >= p.start_secs && t < p.end_secs) return p.phase_id;
      return null;
    }

    // The world rectangle on screen and how it maps to pixels. World y runs
    // down the screen, so a facing of 0 points up.
    function viewport(w, h) {
      const m = maps[shownMap];
      let box = fit;
      let flipX = false;
      let flipY = false;
      if (m) {
        const r = m.view || m;
        box = {
          cx: (r.min_x + r.max_x) / 2,
          cy: (r.min_y + r.max_y) / 2,
          w: Math.abs(r.max_x - r.min_x),
          h: Math.abs(r.max_y - r.min_y),
        };
        // An entry whose min is above its max is drawn mirrored on that axis
        flipX = m.min_x > m.max_x;
        flipY = m.min_y > m.max_y;
      }
      const k = Math.min(w / box.w, h / box.h) * scale;
      return {
        k,
        sx: (x) => w / 2 + (flipX ? -1 : 1) * (x - box.cx) * k + pan.x,
        sy: (y) => h / 2 + (flipY ? -1 : 1) * (y - box.cy) * k + pan.y,
        flipX,
        flipY,
      };
    }

    function drawGrid(v, w, h) {
      const step = GRID_STEPS.find((s) => s * v.k >= 40) || 1000;
      ctx.strokeStyle = colors.grid;
      ctx.lineWidth = 1;
      ctx.beginPath();
      const x0 = v.sx(0) % (step * v.k);
      for (let x = x0; x < w; x += step * v.k) {
        ctx.moveTo(Math.round(x) + 0.5, 0);
        ctx.lineTo(Math.round(x) + 0.5, h);
      }
      const y0 = v.sy(0) % (step * v.k);
      for (let y = y0; y < h; y += step * v.k) {
        ctx.moveTo(0, Math.round(y) + 0.5);
        ctx.lineTo(w, Math.round(y) + 0.5);
      }
      ctx.stroke();
      ctx.fillStyle = colors.muted;
      ctx.font = "11px sans-serif";
      ctx.textAlign = "left";
      ctx.fillText(`grid ${step} m`, 8, h - 8);
    }

    function drawMap(v, m, img) {
      if (!img || !img.complete || !img.naturalWidth) return;
      const x0 = v.sx(m.min_x);
      const x1 = v.sx(m.max_x);
      const y0 = v.sy(m.min_y);
      const y1 = v.sy(m.max_y);
      ctx.save();
      if (m.rotation) {
        ctx.translate((x0 + x1) / 2, (y0 + y1) / 2);
        ctx.rotate((m.rotation * Math.PI) / 180);
        ctx.translate(-(x0 + x1) / 2, -(y0 + y1) / 2);
      }
      // min → left/top edge of the image, whichever way the axis runs on screen
      ctx.drawImage(img, Math.min(x0, x1), Math.min(y0, y1), Math.abs(x1 - x0), Math.abs(y1 - y0));
      ctx.restore();
    }

    function drawEntity(e, s, v, selected) {
      const x = v.sx(s.x);
      const y = v.sy(s.y);
      const npc = e.kind === "Npc";
      const r = npc ? (e.is_boss ? 13 : 8) : 12;
      ctx.globalAlpha = s.dead ? 0.35 : 1;

      // Facing arc: world direction (-sin f, -cos f)
      const f = (s.facing * Math.PI) / 180;
      const dir = Math.atan2((v.flipY ? 1 : -1) * Math.cos(f), (v.flipX ? 1 : -1) * Math.sin(f));
      ctx.strokeStyle = npc ? colors.npc : colors.player;
      ctx.lineWidth = 3;
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.arc(x, y, r + 5, dir - Math.PI / 4, dir + Math.PI / 4);
      ctx.stroke();

      const img = icons.get(e.id);
      ctx.beginPath();
      ctx.arc(x, y, r, 0, TAU);
      if (npc) {
        ctx.fillStyle = colors.npc;
        ctx.globalAlpha *= 0.75;
        ctx.fill();
        ctx.globalAlpha = s.dead ? 0.35 : 1;
      } else if (img && img.complete && img.naturalWidth) {
        ctx.save();
        ctx.clip();
        ctx.fillStyle = colors.bg;
        ctx.fill();
        ctx.drawImage(img, x - r, y - r, r * 2, r * 2);
        ctx.restore();
      } else {
        ctx.fillStyle = colors.player;
        ctx.fill();
      }
      ctx.lineWidth = selected ? 3 : 1.5;
      ctx.strokeStyle = selected ? colors.gold : npc ? colors.npc : colors.text;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, TAU);
      ctx.stroke();

      const number = numbers[e.id];
      if (number) {
        ctx.fillStyle = colors.bg;
        ctx.beginPath();
        ctx.arc(x + r * 0.8, y - r * 0.8, 6.5, 0, TAU);
        ctx.fill();
        ctx.fillStyle = colors.text;
        ctx.font = "bold 10px sans-serif";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(String(number), x + r * 0.8, y - r * 0.8 + 0.5);
        ctx.textBaseline = "alphabetic";
      }
      if (selected || e.is_boss) {
        ctx.fillStyle = colors.text;
        ctx.font = "11px sans-serif";
        ctx.textAlign = "center";
        ctx.fillText(e.name, x, y + r + 17);
      }
      ctx.globalAlpha = 1;
      drawn.push({ id: e.id, x, y, r });
    }

    function draw() {
      raf = 0;
      if (destroyed) return;
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.width / dpr;
      const h = canvas.height / dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = colors.bg;
      ctx.fillRect(0, 0, w, h);

      const phase = maps.length ? phaseAt(time) : null;
      const mapOf = maps.length ? (x, y, z) => mapIndexAt(maps, x, y, z, phase) : null;
      const states = entities.map((e) => stateAt(e, time, duration, mapOf));
      // Which map each entity is on; the shown one follows the living players
      let onMap = null;
      if (mapOf) {
        onMap = states.map((s) => (s ? mapOf(s.x, s.y, s.z) : -1));
        let next = pinned;
        if (next == null) {
          const counts = new Array(maps.length).fill(0);
          entities.forEach((e, i) => {
            if (e.kind === "Player" && states[i] && !states[i].dead && onMap[i] >= 0) counts[onMap[i]]++;
          });
          const best = Math.max(...counts);
          // Ties and an empty raid keep the current map
          next = best > 0 && counts[shownMap] !== best ? counts.indexOf(best) : Math.max(shownMap, 0);
        }
        if (next !== shownMap) {
          shownMap = next;
          pan = { x: 0, y: 0 };
          if (mapCb) mapCb(shownMap);
        }
      }

      const v = viewport(w, h);
      if (maps[shownMap]) drawMap(v, maps[shownMap], mapImages[shownMap]);
      else drawGrid(v, w, h);

      drawn = [];
      const visible = (i) => states[i] && (!onMap || onMap[i] === shownMap);
      // NPCs under players, the highlighted entity on top
      for (const pass of ["Npc", "Companion", "Player"]) {
        entities.forEach((e, i) => {
          if (e.kind === pass && e.id !== highlight && visible(i)) drawEntity(e, states[i], v, false);
        });
      }
      entities.forEach((e, i) => {
        if (e.id === highlight && visible(i)) drawEntity(e, states[i], v, true);
      });
    }

    function requestDraw() {
      if (!raf && !destroyed && !playing) raf = requestAnimationFrame(draw);
    }

    function tick(force) {
      const now = performance.now();
      if (tickCb && (force || now - lastTick >= TICK_MS)) {
        lastTick = now;
        tickCb(time, playing);
      }
    }

    function frame(now) {
      // This frame has run: a paused player must be able to ask for redraws again
      raf = 0;
      if (destroyed || !playing) return;
      const dt = Math.min((now - lastFrame) / 1000, 0.1);
      lastFrame = now;
      time += dt * speed;
      if (time >= duration) {
        time = duration;
        playing = false;
      }
      draw();
      tick(!playing);
      if (playing) raf = requestAnimationFrame(frame);
    }

    // ─── Pointer: drag to pan, click to pick ─────────────────────────────────
    let drag = null;
    function onDown(ev) {
      drag = { x: ev.clientX, y: ev.clientY, pan: { ...pan }, moved: false };
      canvas.setPointerCapture(ev.pointerId);
    }
    function onMove(ev) {
      if (!drag) return;
      const dx = ev.clientX - drag.x;
      const dy = ev.clientY - drag.y;
      if (Math.hypot(dx, dy) > 4) drag.moved = true;
      if (drag.moved) {
        pan = { x: drag.pan.x + dx, y: drag.pan.y + dy };
        requestDraw();
      }
    }
    function onUp(ev) {
      const wasDrag = drag && drag.moved;
      drag = null;
      if (wasDrag || !pickCb) return;
      const rect = canvas.getBoundingClientRect();
      const x = ev.clientX - rect.left;
      const y = ev.clientY - rect.top;
      let best = null;
      // Last drawn is on top
      for (let i = drawn.length - 1; i >= 0; i--) {
        const d = Math.hypot(drawn[i].x - x, drawn[i].y - y);
        if (d <= Math.max(PICK_PX, drawn[i].r) && (!best || d < best.d)) best = { id: drawn[i].id, d };
      }
      pickCb(best ? best.id : null);
    }
    canvas.addEventListener("pointerdown", onDown);
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerup", onUp);
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    resize();

    return {
      play() {
        if (playing || destroyed) return;
        if (time >= duration) time = 0;
        playing = true;
        lastFrame = performance.now();
        cancelAnimationFrame(raf);
        raf = requestAnimationFrame(frame);
        tick(true);
      },
      pause() {
        playing = false;
        cancelAnimationFrame(raf);
        raf = 0;
        tick(true);
        requestDraw();
      },
      seek(secs) {
        time = Math.max(0, Math.min(duration, secs));
        tick(true);
        requestDraw();
      },
      setSpeed(x) {
        speed = x;
      },
      setScale(x) {
        scale = x;
        requestDraw();
      },
      setHighlight(id) {
        highlight = id;
        requestDraw();
      },
      // Map entries with one image URL each (null until loaded) and the phase spans
      setMaps(entries, urls, phaseSpans) {
        maps = entries || [];
        phases = phaseSpans || [];
        mapImages = (urls || []).map((url) => {
          if (!url) return null;
          const img = new Image();
          img.onload = requestDraw;
          img.src = url;
          return img;
        });
        if (shownMap >= maps.length) shownMap = -1;
        requestDraw();
      },
      // Pin a map entry, or null to follow the raid
      setPinnedMap(index) {
        pinned = index;
        requestDraw();
      },
      onTick(cb) {
        tickCb = cb;
      },
      onPick(cb) {
        pickCb = cb;
      },
      onMapChange(cb) {
        mapCb = cb;
      },
      destroy() {
        destroyed = true;
        playing = false;
        cancelAnimationFrame(raf);
        observer.disconnect();
        canvas.removeEventListener("pointerdown", onDown);
        canvas.removeEventListener("pointermove", onMove);
        canvas.removeEventListener("pointerup", onUp);
      },
    };
  }

  return { create, mapIndexAt };
})();
