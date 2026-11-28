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
  // Share of the living raid a map must hold to be switched to, and to be kept
  const ENTER_SHARE = 0.75;
  const STAY_SHARE = 0.5;
  // Following moves in only to a map this many times smaller, and out to one
  // at most this many times larger when a closer fit exists
  const TIGHTER = 2;
  const WIDER = 3;
  // Share of the whole fight's positions that makes a map the fight's home
  const HOME_SHARE = 0.9;
  // …unless it is more than this many times wider than the area the fight used
  const HOME_MAX_SIZE = 3;
  // An entity this far past a map's edge (as a share of its size) is still drawn
  const EDGE_MARGIN = 0.25;
  // Playback seconds a different map must stay the best before it is shown
  const MAP_HOLD_SECS = 1;
  // Zoom range: far enough out to see a whole page from a framed fight, in to a few metres
  const MIN_SCALE = 0.1;
  const MAX_SCALE = 60;
  // Zoom per wheel pixel (one notch is about 100): roughly 15% a notch
  const WHEEL_ZOOM = 0.0015;
  // Dot size follows zoom^power, kept between these factors of the base size
  const DOT_ZOOM_POWER = 0.45;
  const DOT_MIN_ZOOM = 0.8;
  const DOT_MAX_ZOOM = 3;
  // The selected dot is this much larger, and the others this opaque
  const SELECTED_GROW = 1.3;
  const UNSELECTED_ALPHA = 0.7;
  // Opening a map framed on the fight: room around the positions, a floor on
  // how tight it gets (metres), and skipped when it would show most of the page
  const FOCUS_PADDING = 2.15;
  const FOCUS_MIN_SIZE = 80;
  const FOCUS_MAX_SHARE = 0.7;
  const FOCUS_MIN_SAMPLES = 20;
  // Pages up to this many metres across are never framed
  const FOCUS_MIN_PAGE = 250;
  // The highlighted entity's path over this many seconds, sampled this often
  const TRAIL_SECS = 6;
  const TRAIL_STEP_SECS = 0.25;
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

  // Entity id an entity has targeted at t, or null
  function targetAt(e, t) {
    let lo = 0;
    let hi = e.targets.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (e.targets[mid][0] <= t) lo = mid + 1;
      else hi = mid;
    }
    return lo ? e.targets[lo - 1][1] : null;
  }

  function isDead(e, t) {
    return e.dead.some(([from, to]) => t >= from && t < to);
  }

  // Whether a map entry covers a world position at a phase
  function mapContains(m, x, y, z, phase) {
    const [x0, x1] = m.min_x < m.max_x ? [m.min_x, m.max_x] : [m.max_x, m.min_x];
    const [y0, y1] = m.min_y < m.max_y ? [m.min_y, m.max_y] : [m.max_y, m.min_y];
    if (x < x0 || x > x1 || y < y0 || y > y1) return false;
    if (m.min_z != null && z < m.min_z) return false;
    if (m.max_z != null && z > m.max_z) return false;
    return !(m.phases && m.phases.length) || m.phases.includes(phase);
  }

  // Whether a position is on a map's floor and on or just past its image
  function mapNear(m, x, y, z, phase) {
    const dx = Math.abs(m.max_x - m.min_x) * EDGE_MARGIN;
    const dy = Math.abs(m.max_y - m.min_y) * EDGE_MARGIN;
    const grown = {
      ...m,
      min_x: Math.min(m.min_x, m.max_x) - dx,
      max_x: Math.max(m.min_x, m.max_x) + dx,
      min_y: Math.min(m.min_y, m.max_y) - dy,
      max_y: Math.max(m.min_y, m.max_y) + dy,
    };
    return mapContains(grown, x, y, z, phase);
  }

  // Position, facing and presence of an entity at t; null when it is not there.
  // `onShown(x, y, z)` says whether the map on screen covers a position.
  function stateAt(e, t, duration, onShown) {
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
    // Never glide onto or off the map on screen
    if (onShown && onShown(s.x, s.y, s.z) !== onShown(tr.x[j], tr.y[j], tr.z[j])) return s;
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
    const roles = meta.roles || {};
    const byId = new Map(entities.map((e, i) => [e.id, i]));
    const fit = fitBounds(entities);

    let time = 0;
    let playing = false;
    let speed = 1;
    let scale = 1;
    let pan = { x: 0, y: 0 };
    let highlight = null;
    let raf = 0;
    // A span of the fight to play over and over ({ from, to }), or null
    let loop = null;
    let lastFrame = 0;
    let lastTick = 0;
    let tickCb = null;
    let pickCb = null;
    let mapCb = null;
    let scaleCb = null;
    // Nameless NPCs are invisible helpers that mechanics are scripted on
    let showUnnamed = false;
    let destroyed = false;
    let colors = {};
    // Maps: entries, their images, phase spans, the pinned entry (null = follow the raid)
    let maps = [];
    let mapImages = [];
    let phases = [];
    let pinned = null;
    let shownMap = -1;
    // Auto-follow: the map waiting to be shown and since when; a seek settles at once
    let pendingMap = -1;
    let pendingAt = 0;
    let settleMap = true;
    // Entry the fight mostly happens on, or -1 when no single entry covers it
    let homeMap = -1;
    // Per map entry: the area the fight used, when it is worth zooming to
    let focus = [];
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
        tank: v("--swtor-blue", "#4a9eff"),
        heal: v("--color-success", "#5fd98e"),
        dps: v("--swtor-orange", "#ff8c42"),
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

    // The part of a map page the fight actually used (players and bosses),
    // with room around it, or null when the page is room-sized or mostly used
    function fightArea(m) {
      const [px0, px1] = [Math.min(m.min_x, m.max_x), Math.max(m.min_x, m.max_x)];
      const [py0, py1] = [Math.min(m.min_y, m.max_y), Math.max(m.min_y, m.max_y)];
      // A page this small is already scoped to one room: show all of it
      if (Math.max(px1 - px0, py1 - py0) <= FOCUS_MIN_PAGE) return null;
      const xs = [];
      const ys = [];
      const page = { ...m, phases: null };
      for (const e of entities) {
        if (e.kind !== "Player" && !e.is_boss) continue;
        const tr = e.track;
        for (let i = 0; i < tr.t.length; i++) {
          if (mapContains(page, tr.x[i], tr.y[i], tr.z[i], null)) {
            xs.push(tr.x[i]);
            ys.push(tr.y[i]);
          }
        }
      }
      if (xs.length < FOCUS_MIN_SAMPLES) return null;
      const span = (values, lo, hi) => {
        values.sort((a, b) => a - b);
        // Ignore the odd stray position
        const a = values[Math.floor(values.length * 0.02)];
        const b = values[Math.ceil(values.length * 0.98) - 1];
        const half = Math.max((b - a) * FOCUS_PADDING, FOCUS_MIN_SIZE) / 2;
        const mid = (a + b) / 2;
        return [Math.max(lo, mid - half), Math.min(hi, mid + half)];
      };
      const [x0, x1] = span(xs, px0, px1);
      const [y0, y1] = span(ys, py0, py1);
      // One scale for both axes: the larger need decides
      const used = Math.max((x1 - x0) / (px1 - px0), (y1 - y0) / (py1 - py0));
      if (used >= FOCUS_MAX_SHARE) return null;
      return { min_x: x0, max_x: x1, min_y: y0, max_y: y1 };
    }

    // Which map to show while following the raid. Entries overlap (a room, the
    // hall around it, the whole instance) and are listed smallest first.
    function followRaid(states, phase) {
      const counts = new Array(maps.length).fill(0);
      let alive = 0;
      entities.forEach((e, i) => {
        const s = states[i];
        if (e.kind !== "Player" || !s || s.dead) return;
        alive++;
        for (let m = 0; m < maps.length; m++) if (mapContains(maps[m], s.x, s.y, s.z, phase)) counts[m]++;
      });
      const current = Math.max(shownMap, 0);
      if (!alive) return current;
      const holds = (m, share) => counts[m] >= alive * share;
      // The fight's home page stays while the raid is mostly on it
      if (homeMap >= 0 && (holds(homeMap, ENTER_SHARE) || (shownMap === homeMap && holds(homeMap, STAY_SHARE)))) {
        return homeMap;
      }
      const size = (m) => Math.max(Math.abs(maps[m].max_x - maps[m].min_x), Math.abs(maps[m].max_y - maps[m].min_y));
      const room = counts.findIndex((n, m) => holds(m, ENTER_SHARE));
      if (shownMap < 0) return room >= 0 ? room : current;
      if (holds(current, STAY_SHARE)) {
        // Still here: only move in to a much tighter map
        return room >= 0 && size(room) * TIGHTER <= size(current) ? room : current;
      }
      // The raid left this map. Prefer a map of a similar scale over jumping
      // out to the whole instance.
      if (room >= 0 && size(room) <= size(current) * WIDER) return room;
      let similar = -1;
      for (let m = 0; m < maps.length; m++) {
        if (size(m) <= size(current) * WIDER && holds(m, STAY_SHARE) && (similar < 0 || counts[m] > counts[similar])) {
          similar = m;
        }
      }
      if (similar >= 0) return similar;
      return room >= 0 ? room : current;
    }

    // The world rectangle on screen and how it maps to pixels. World y runs
    // down the screen, so a facing of 0 points up.
    function viewport(w, h) {
      const m = maps[shownMap];
      let box = fit;
      let flipX = false;
      let flipY = false;
      if (m) {
        // A boss's own view wins; else the part of the page the fight used
        const r = m.view || focus[shownMap] || m;
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

    function drawTargetLine(v, from, to, color) {
      ctx.save();
      ctx.globalAlpha = 0.6;
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.5;
      ctx.setLineDash([6, 4]);
      ctx.beginPath();
      ctx.moveTo(v.sx(from.x), v.sy(from.y));
      ctx.lineTo(v.sx(to.x), v.sy(to.y));
      ctx.stroke();
      ctx.restore();
    }

    // Where an entity walked over the last seconds, fading with age. `shown`
    // tells whether a past position is on the map being drawn.
    function drawTrail(e, v, onShown, shown) {
      ctx.save();
      ctx.strokeStyle = colors.gold;
      ctx.lineWidth = 2.5;
      ctx.lineCap = "round";
      let prev = null;
      // Points sit at fixed moments of the fight, so the line stays put as time
      // moves: it only grows at the head and fades at the tail
      const first = Math.ceil(Math.max(0, time - TRAIL_SECS) / TRAIL_STEP_SECS);
      const last = Math.floor(time / TRAIL_STEP_SECS);
      for (let k = first; k <= last + 1; k++) {
        const at = k > last ? time : k * TRAIL_STEP_SECS;
        const s = stateAt(e, at, duration, onShown);
        const point = s && !s.dead && shown(s) ? s : null;
        // A teleport is not a walk
        if (prev && point && Math.hypot(point.x - prev.x, point.y - prev.y) <= SNAP_DIST) {
          ctx.globalAlpha = 0.75 * (1 - (time - at) / TRAIL_SECS);
          ctx.beginPath();
          ctx.moveTo(v.sx(prev.x), v.sy(prev.y));
          ctx.lineTo(v.sx(point.x), v.sy(point.y));
          ctx.stroke();
        }
        prev = point;
      }
      ctx.restore();
    }

    function drawEntity(e, s, v, selected) {
      const x = v.sx(s.x);
      const y = v.sy(s.y);
      const npc = e.kind === "Npc";
      // Dots grow as the map is zoomed in, but far slower than the map does
      const zoom = Math.max(DOT_MIN_ZOOM, Math.min(DOT_MAX_ZOOM, Math.pow(scale, DOT_ZOOM_POWER)));
      const r = (npc ? (e.is_boss ? 13 : 8) : 12) * zoom * (selected ? SELECTED_GROW : 1);
      // With someone selected, everyone else steps back
      const alpha = (s.dead ? 0.35 : 1) * (highlight != null && !selected ? UNSELECTED_ALPHA : 1);
      ctx.globalAlpha = alpha;
      if (selected) {
        // A soft gold halo, so the selection reads at a glance
        ctx.fillStyle = colors.gold;
        ctx.globalAlpha = 0.28;
        ctx.beginPath();
        ctx.arc(x, y, r + 11 * zoom, 0, TAU);
        ctx.fill();
        ctx.globalAlpha = alpha;
      }

      // Facing arc: world direction (-sin f, -cos f)
      const f = (s.facing * Math.PI) / 180;
      const dir = Math.atan2((v.flipY ? 1 : -1) * Math.cos(f), (v.flipX ? 1 : -1) * Math.sin(f));
      // Players are ringed by role: tank, healer or damage
      const tint = npc ? colors.npc : colors[roles[e.id]] || colors.player;
      ctx.strokeStyle = tint;
      ctx.lineWidth = 3 * zoom;
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.arc(x, y, r + 5 * zoom, dir - Math.PI / 4, dir + Math.PI / 4);
      ctx.stroke();

      const img = icons.get(e.id);
      ctx.beginPath();
      ctx.arc(x, y, r, 0, TAU);
      if (npc) {
        ctx.fillStyle = colors.npc;
        ctx.globalAlpha *= 0.75;
        ctx.fill();
        ctx.globalAlpha = alpha;
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
      if (selected) {
        // Dark edge under the gold ring keeps it clear of the map's own lines
        ctx.lineWidth = 7;
        ctx.strokeStyle = colors.bg;
        ctx.beginPath();
        ctx.arc(x, y, r, 0, TAU);
        ctx.stroke();
      }
      ctx.lineWidth = selected ? 4 : 2;
      ctx.strokeStyle = selected ? colors.gold : tint;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, TAU);
      ctx.stroke();

      const number = numbers[e.id];
      if (number) {
        ctx.fillStyle = colors.bg;
        ctx.beginPath();
        ctx.arc(x + r * 0.8, y - r * 0.8, 6.5 * zoom, 0, TAU);
        ctx.fill();
        ctx.fillStyle = colors.text;
        ctx.font = `bold ${Math.round(10 * zoom)}px sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(String(number), x + r * 0.8, y - r * 0.8 + 0.5);
        ctx.textBaseline = "alphabetic";
      }
      if (selected || e.is_boss) {
        ctx.fillStyle = colors.text;
        ctx.font = "11px sans-serif";
        ctx.textAlign = "center";
        ctx.fillText(e.name, x, y + r + 5 * zoom + 12);
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
      let states = entities.map((e) => stateAt(e, time, duration, null));
      let onShown = null;
      if (maps.length) {
        let next = pinned;
        if (next == null) {
          next = followRaid(states, phase);
          // A few players through a doorway must not flip the map: a change
          // has to hold for a moment, except right after a seek
          if (next !== shownMap && shownMap >= 0 && !settleMap) {
            if (next !== pendingMap) {
              pendingMap = next;
              pendingAt = time;
            }
            if (Math.abs(time - pendingAt) < MAP_HOLD_SECS) next = shownMap;
          }
        }
        settleMap = false;
        if (next !== shownMap) {
          pendingMap = -1;
          shownMap = next;
          pan = { x: 0, y: 0 };
          if (mapCb) mapCb(shownMap);
        }
        onShown = (x, y, z) => mapNear(maps[shownMap], x, y, z, phase);
        states = entities.map((e) => stateAt(e, time, duration, onShown));
      }

      const v = viewport(w, h);
      if (maps[shownMap]) drawMap(v, maps[shownMap], mapImages[shownMap]);
      else drawGrid(v, w, h);

      drawn = [];
      const visible = (i) =>
        states[i] &&
        (showUnnamed || entities[i].name) &&
        (!maps.length || mapNear(maps[shownMap], states[i].x, states[i].y, states[i].z, phase));
      // Who a boss is on, and who the highlighted entity is on
      entities.forEach((e, i) => {
        if ((e.is_boss || e.id === highlight) && visible(i) && !states[i].dead) {
          const to = byId.get(targetAt(e, time));
          if (to != null && to !== i && visible(to)) {
            drawTargetLine(v, states[i], states[to], e.id === highlight ? colors.gold : colors.npc);
          }
        }
      });
      entities.forEach((e, i) => {
        if (e.id === highlight && visible(i)) {
          drawTrail(e, v, onShown, (s) => !onShown || onShown(s.x, s.y, s.z));
        }
      });
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
      if (loop && time >= loop.to) {
        time = loop.from;
        settleMap = true;
      } else if (time >= duration) {
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
    // Zoom keeping one screen point fixed: the cursor, or the centre
    function zoomTo(next, at) {
      next = Math.max(MIN_SCALE, Math.min(MAX_SCALE, next));
      const rect = canvas.getBoundingClientRect();
      const ax = at ? at.x - rect.width / 2 : 0;
      const ay = at ? at.y - rect.height / 2 : 0;
      const ratio = next / scale;
      pan = { x: ax - (ax - pan.x) * ratio, y: ay - (ay - pan.y) * ratio };
      scale = next;
      requestDraw();
    }
    function onWheel(ev) {
      ev.preventDefault();
      const rect = canvas.getBoundingClientRect();
      zoomTo(scale * Math.exp(-ev.deltaY * WHEEL_ZOOM), { x: ev.clientX - rect.left, y: ev.clientY - rect.top });
      if (scaleCb) scaleCb(scale);
    }
    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("pointerdown", onDown);
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerup", onUp);
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    // The theme toggle swaps the colours the canvas reads
    const themeObserver = new MutationObserver(() => {
      readColors();
      requestDraw();
    });
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    resize();

    return {
      play() {
        if (playing || destroyed) return;
        if (time >= duration) time = loop ? loop.from : 0;
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
        time = Math.max(loop ? loop.from : 0, Math.min(loop ? loop.to : duration, secs));
        settleMap = true;
        tick(true);
        requestDraw();
      },
      setSpeed(x) {
        speed = x;
      },
      // Play only from..to, over and over; null for the whole fight
      setLoop(from, to) {
        const clamped = from == null || to == null ? null : { from: Math.max(0, from), to: Math.min(duration, to) };
        loop = clamped && clamped.to > clamped.from ? clamped : null;
        if (loop && (time < loop.from || time > loop.to)) {
          time = loop.from;
          settleMap = true;
        }
        tick(true);
        requestDraw();
      },
      setScale(x) {
        zoomTo(x, null);
      },
      setShowUnnamed(on) {
        showUnnamed = on;
        requestDraw();
      },
      // Default zoom, recentred
      resetView() {
        scale = 1;
        pan = { x: 0, y: 0 };
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
        // Home: the smallest entry covering nearly all of where the players stood
        let total = 0;
        const covered = new Array(maps.length).fill(0);
        for (const e of entities) {
          if (e.kind !== "Player") continue;
          const tr = e.track;
          for (let i = 0; i < tr.t.length; i++) {
            total++;
            for (let m = 0; m < maps.length; m++) {
              if (mapContains({ ...maps[m], phases: null }, tr.x[i], tr.y[i], tr.z[i], null)) covered[m]++;
            }
          }
        }
        // A page far wider than the fight itself (the whole instance) is no home
        const reach = Math.max(fit.w, fit.h) * HOME_MAX_SIZE;
        const size = (m) => Math.max(Math.abs(m.max_x - m.min_x), Math.abs(m.max_y - m.min_y));
        homeMap = covered.findIndex((n, m) => total > 0 && n >= total * HOME_SHARE && size(maps[m]) <= reach);
        focus = maps.map((m) => fightArea(m));
        settleMap = true;
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
      // Called when the wheel changed the zoom, so the slider can follow
      onScale(cb) {
        scaleCb = cb;
      },
      onMapChange(cb) {
        mapCb = cb;
      },
      destroy() {
        destroyed = true;
        playing = false;
        cancelAnimationFrame(raf);
        observer.disconnect();
        themeObserver.disconnect();
        canvas.removeEventListener("wheel", onWheel);
        canvas.removeEventListener("pointerdown", onDown);
        canvas.removeEventListener("pointermove", onMove);
        canvas.removeEventListener("pointerup", onUp);
      },
    };
  }

  return { create };
})();
