const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const state = {
  view: "library",
  settings: null,
  instances: [],
  versions: [],
  starting: new Set(),
  running: new Set(),
  logs: {},
  openLog: null,
  discover: { type: "mod", query: "", instanceId: null, hits: [], total: 0, offset: 0, loading: false },
  cosmetics: { catalog: { cosmetics: [] }, equipped: [] },
};

// ---------- helpers ----------
const $ = (s, r = document) => r.querySelector(s);

function h(tag, props = {}, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v === false || v == null) continue;
    if (k === "class") el.className = v;
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "value") el.value = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const kid of kids.flat()) el.append(kid instanceof Node ? kid : document.createTextNode(kid ?? ""));
  return el;
}

const compact = new Intl.NumberFormat("de-DE", { notation: "compact" });

function toast(msg, isErr = false) {
  const t = h("div", { class: "toast" + (isErr ? " err" : "") }, msg);
  $("#toasts").append(t);
  setTimeout(() => t.remove(), isErr ? 8000 : 4000);
}

async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    toast(String(e), true);
    throw e;
  }
}

function loaderLabel(i) {
  return i.loader === "fabric" ? `Fabric ${i.loader_version ?? ""}`.trim() : "Vanilla";
}

// ---------- progress ----------
let progressTimer;
listen("progress", (e) => {
  const p = e.payload;
  const box = $("#progress");
  box.classList.remove("hidden");
  $(".progress-text", box).textContent = p.total > 1 ? `${p.stage} – ${p.done}/${p.total}` : p.stage;
  $(".progress-fill", box).style.width = (p.total ? (p.done / p.total) * 100 : 0) + "%";
  clearTimeout(progressTimer);
  progressTimer = setTimeout(() => box.classList.add("hidden"), 2500);
});

listen("game-log", (e) => {
  const { id, line } = e.payload;
  const logs = (state.logs[id] ??= []);
  logs.push(line);
  if (logs.length > 600) logs.shift();
  if (state.openLog === id) {
    const box = $("#logbox");
    if (box) {
      box.textContent = logs.join("\n");
      box.scrollTop = box.scrollHeight;
    }
  }
});

listen("game-exit", (e) => {
  const { id, code } = e.payload;
  state.running.delete(id);
  toast(code === 0 ? "Spiel beendet." : `Spiel beendet (Code ${code ?? "?"}).`, code !== 0 && code !== null);
  if (state.view === "library") render();
});

// ---------- navigation ----------
const NAV = [
  ["library", "Bibliothek", "M4 5h6v6H4zM14 5h6v6h-6zM4 15h6v4H4zM14 15h6v4h-6z"],
  ["discover", "Entdecken", "M11 4a7 7 0 105 12l4 4 1.5-1.5-4-4A7 7 0 0011 4z"],
  ["cosmetics", "Cosmetics", "M12 3l2.6 5.6 6 .8-4.4 4.2 1.1 6L12 16.7 6.7 19.6l1.1-6L3.4 9.4l6-.8z"],
  ["settings", "Einstellungen", "M12 8a4 4 0 100 8 4 4 0 000-8zM3 13v-2l2.2-.5.6-1.4-1.2-1.9 1.4-1.4 1.9 1.2 1.4-.6L10 3h2l.5 2.2 1.4.6 1.9-1.2 1.4 1.4-1.2 1.9.6 1.4L21 11v2l-2.2.5-.6 1.4 1.2 1.9-1.4 1.4-1.9-1.2-1.4.6L12 21h-2l-.5-2.2-1.4-.6-1.9 1.2-1.4-1.4 1.2-1.9-.6-1.4z"],
];

function renderNav() {
  const nav = $("#nav");
  nav.replaceChildren(
    ...NAV.map(([id, label, path]) =>
      h(
        "button",
        { class: "nav-item" + (state.view === id ? " active" : ""), onclick: () => go(id) },
        (() => {
          const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
          svg.setAttribute("viewBox", "0 0 24 24");
          svg.setAttribute("width", "18");
          svg.setAttribute("height", "18");
          svg.setAttribute("fill", "currentColor");
          const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
          p.setAttribute("d", path);
          svg.append(p);
          return svg;
        })(),
        label
      )
    )
  );
  $("#who").replaceChildren("Spielt als ", h("b", {}, state.settings?.username ?? "…"), " (offline)");
}

function go(view) {
  state.view = view;
  render();
}

async function render() {
  renderNav();
  const main = $("#main");
  const views = { library: viewLibrary, discover: viewDiscover, cosmetics: viewCosmetics, settings: viewSettings };
  main.replaceChildren(await views[state.view]());
}

// ---------- library ----------
async function reloadInstances() {
  state.instances = await call("list_instances");
}

async function viewLibrary() {
  await reloadInstances();
  const root = h("div");
  root.append(
    h(
      "div",
      { class: "row between" },
      h("div", {}, h("h1", {}, "Bibliothek"), h("p", { class: "sub" }, "Deine Instanzen")),
      h("button", { class: "btn primary", onclick: openCreateModal }, "+ Neue Instanz")
    )
  );
  if (!state.instances.length) {
    root.append(h("div", { class: "empty" }, "Noch keine Instanz. Leg eine an oder installier ein Modpack unter „Entdecken“."));
    return root;
  }
  const grid = h("div", { class: "grid" });
  for (const i of state.instances) {
    const busy = state.starting.has(i.id);
    const running = state.running.has(i.id);
    grid.append(
      h(
        "div",
        { class: "card inst" },
        h(
          "div",
          { class: "inst-head" },
          h("div", {}, h("h3", {}, i.name), h("div", { class: "meta" }, `Minecraft ${i.mc_version} · ${loaderLabel(i)}`)),
          running ? h("span", { class: "tag" }, "läuft") : ""
        ),
        h(
          "div",
          { class: "row" },
          h("button", { class: "btn primary", disabled: busy || running, onclick: () => play(i.id) }, busy ? "Startet…" : running ? "Läuft" : "Spielen"),
          h("button", { class: "btn small", onclick: () => toggleLog(i.id) }, "Log"),
          h("button", { class: "btn small danger", onclick: () => removeInstance(i) }, "Löschen")
        ),
        state.openLog === i.id ? h("div", { class: "logbox", id: "logbox" }, (state.logs[i.id] ?? []).join("\n")) : ""
      )
    );
  }
  root.append(grid);
  return root;
}

function toggleLog(id) {
  state.openLog = state.openLog === id ? null : id;
  render();
}

async function play(id) {
  state.starting.add(id);
  state.openLog = id;
  state.logs[id] = [];
  render();
  try {
    await invoke("play", { id });
    state.running.add(id);
  } catch (e) {
    toast(String(e), true);
  } finally {
    state.starting.delete(id);
    if (state.view === "library") render();
  }
}

async function removeInstance(i) {
  if (!confirm(`„${i.name}“ samt Welten und Mods löschen?`)) return;
  await call("delete_instance", { id: i.id });
  render();
}

async function openCreateModal() {
  if (!state.versions.length) {
    try {
      state.versions = await call("mc_versions", { snapshots: false });
    } catch {
      return;
    }
  }
  const name = h("input", { type: "text", placeholder: "Meine Instanz" });
  const version = h("select", {}, state.versions.map((v) => h("option", { value: v.id }, v.id)));
  const loader = h("select", {}, h("option", { value: "fabric" }, "Fabric"), h("option", { value: "vanilla" }, "Vanilla"));
  const close = () => $("#modal-root").replaceChildren();
  $("#modal-root").replaceChildren(
    h(
      "div",
      { class: "modal-bg", onclick: (e) => e.target.classList.contains("modal-bg") && close() },
      h(
        "div",
        { class: "modal" },
        h("h2", {}, "Neue Instanz"),
        h("label", { class: "field" }, "Name", name),
        h("label", { class: "field" }, "Minecraft-Version", version),
        h("label", { class: "field" }, "Loader", loader),
        h(
          "div",
          { class: "row between" },
          h("button", { class: "btn", onclick: close }, "Abbrechen"),
          h(
            "button",
            {
              class: "btn primary",
              onclick: async () => {
                await call("create_instance", { name: name.value || `Minecraft ${version.value}`, mcVersion: version.value, loader: loader.value });
                close();
                render();
              },
            },
            "Erstellen"
          )
        )
      )
    )
  );
  name.focus();
}

// ---------- discover ----------
const TYPES = [
  ["mod", "Mods"],
  ["modpack", "Modpacks"],
  ["resourcepack", "Resource Packs"],
  ["shader", "Shader"],
];

async function viewDiscover() {
  await reloadInstances();
  const d = state.discover;
  if (!d.instanceId || !state.instances.find((i) => i.id === d.instanceId)) d.instanceId = state.instances[0]?.id ?? null;

  const root = h("div");
  root.append(h("h1", {}, "Entdecken"), h("p", { class: "sub" }, "Mods, Modpacks, Resource Packs und Shader von Modrinth"));

  root.append(
    h(
      "div",
      { class: "tabs" },
      TYPES.map(([id, label]) =>
        h("button", { class: "tab" + (d.type === id ? " active" : ""), onclick: () => { d.type = id; d.hits = []; d.offset = 0; searchModrinth(true); render(); } }, label)
      )
    )
  );

  const q = h("input", { type: "search", placeholder: "Suchen…", value: d.query, style: "flex:1;min-width:200px" });
  let t;
  q.addEventListener("input", () => {
    clearTimeout(t);
    t = setTimeout(() => { d.query = q.value; searchModrinth(true); }, 350);
  });
  const row = h("div", { class: "row", style: "margin-bottom:16px" }, q);
  if (d.type !== "modpack") {
    const sel = h("select", { onchange: () => { d.instanceId = sel.value; searchModrinth(true); } },
      state.instances.map((i) => h("option", { value: i.id, selected: i.id === d.instanceId }, `${i.name} (${i.mc_version}, ${loaderLabel(i)})`)));
    row.append(h("span", { class: "meta" }, "Für:"), sel);
  }
  root.append(row);

  if (d.type !== "modpack" && !state.instances.length) {
    root.append(h("div", { class: "empty" }, "Lege zuerst eine Instanz an, dann kannst du Inhalte installieren."));
    return root;
  }

  root.append(h("div", { id: "hits" }, renderHits()));
  if (!d.hits.length && !d.loading) searchModrinth(true);
  return root;
}

function renderHits() {
  const d = state.discover;
  const wrap = h("div");
  if (d.loading && !d.hits.length) wrap.append(h("div", { class: "empty" }, "Lade…"));
  for (const hit of d.hits) {
    wrap.append(
      h(
        "div",
        { class: "hit" },
        hit.icon_url ? h("img", { src: hit.icon_url, alt: "" }) : h("div", { class: "ph" }),
        h("div", {}, h("h3", {}, hit.title), h("div", { class: "meta" }, `von ${hit.author} · ${compact.format(hit.downloads)} Downloads`), h("p", {}, hit.description)),
        h("button", { class: "btn primary", onclick: (e) => install(hit, e.currentTarget) }, "Installieren")
      )
    );
  }
  if (d.hits.length && d.hits.length < d.total) {
    wrap.append(h("div", { class: "row", style: "justify-content:center;margin-top:8px" }, h("button", { class: "btn", onclick: () => { d.offset += 20; searchModrinth(false); } }, "Mehr laden")));
  }
  if (!d.loading && !d.hits.length) wrap.append(h("div", { class: "empty" }, "Nichts gefunden."));
  return wrap;
}

async function searchModrinth(reset) {
  const d = state.discover;
  if (reset) { d.offset = 0; d.hits = []; }
  d.loading = true;
  const hits = $("#hits");
  if (hits) hits.replaceChildren(renderHits());
  const inst = state.instances.find((i) => i.id === d.instanceId);
  try {
    const r = await invoke("modrinth_search", {
      query: d.query,
      projectType: d.type,
      mcVersion: d.type === "modpack" ? null : inst?.mc_version ?? null,
      loader: d.type === "mod" ? inst?.loader ?? null : null,
      offset: d.offset,
    });
    d.hits = reset ? r.hits : d.hits.concat(r.hits);
    d.total = r.total;
  } catch (e) {
    toast(String(e), true);
  } finally {
    d.loading = false;
    const hits2 = $("#hits");
    if (hits2) hits2.replaceChildren(renderHits());
  }
}

async function install(hit, btn) {
  const d = state.discover;
  btn.disabled = true;
  btn.textContent = "Installiere…";
  try {
    if (d.type === "modpack") {
      const inst = await invoke("modrinth_install_modpack", { projectId: hit.project_id });
      toast(`Instanz „${inst.name}“ erstellt – beim ersten Start lädt Atlas Minecraft nach.`);
    } else {
      const files = await invoke("modrinth_install", { projectId: hit.project_id, projectType: d.type, instanceId: d.instanceId });
      toast(`Installiert: ${files.join(", ")}`);
    }
    btn.textContent = "Installiert ✓";
  } catch (e) {
    toast(String(e), true);
    btn.disabled = false;
    btn.textContent = "Installieren";
  }
}

// ---------- cosmetics ----------
async function viewCosmetics() {
  state.cosmetics = await call("cosmetics_list");
  const root = h("div");
  root.append(
    h(
      "div",
      { class: "row between" },
      h("div", {}, h("h1", {}, "Cosmetics"), h("p", { class: "sub" }, "Pro Slot ein Cosmetic. Der Katalog wächst über Updates.")),
      h("button", { class: "btn", onclick: syncCosmetics }, "Synchronisieren")
    ),
    h("div", { class: "banner" }, "Ausgerüstete Cosmetics gibt Atlas beim Start an das Spiel weiter. Gezeichnet werden sie vom Atlas-Cosmetics-Mod, der noch folgt.")
  );
  const list = state.cosmetics.catalog.cosmetics;
  if (!list.length) {
    root.append(h("div", { class: "empty" }, "Kein Katalog geladen – tippe auf „Synchronisieren“."));
    return root;
  }
  const grid = h("div", { class: "grid" });
  for (const c of list) {
    const on = state.cosmetics.equipped.includes(c.id);
    grid.append(
      h(
        "div",
        { class: "card cos-card" + (on ? " equipped" : "") },
        c.preview_data ? h("img", { class: "cos-preview", src: c.preview_data, alt: "" }) : h("div", { class: "cos-preview" }),
        h("div", { class: "row between" }, h("h3", {}, c.name), h("span", { class: "tag " + c.rarity }, c.rarity)),
        h("div", { class: "meta" }, `${c.slot}${c.description ? " · " + c.description : ""}`),
        h("button", { class: "btn " + (on ? "" : "primary"), onclick: async () => { await call(on ? "cosmetics_unequip" : "cosmetics_equip", { id: c.id }); render(); } }, on ? "Ablegen" : "Ausrüsten")
      )
    );
  }
  root.append(grid);
  return root;
}

async function syncCosmetics() {
  try {
    await invoke("cosmetics_sync");
    toast("Cosmetics aktualisiert.");
  } catch (e) {
    toast(String(e), true);
  }
  render();
}

// ---------- settings ----------
async function viewSettings() {
  const s = (state.settings = await call("get_settings"));
  const username = h("input", { type: "text", value: s.username });
  const ram = h("input", { type: "number", min: "1024", max: "32768", step: "512", value: String(s.ram_mb) });
  const java = h("input", { type: "text", value: s.java_path, placeholder: "Leer lassen: Atlas lädt passendes Java selbst" });
  const url = h("input", { type: "text", value: s.cosmetics_url });
  return h(
    "div",
    { style: "max-width:520px" },
    h("h1", {}, "Einstellungen"),
    h("p", { class: "sub" }, "Offline-Modus: Der Name gilt nur lokal und auf Offline-Servern. Microsoft-Login folgt."),
    h("label", { class: "field" }, "Spielername", username),
    h("label", { class: "field" }, "Arbeitsspeicher (MB)", ram),
    h("label", { class: "field" }, "Java-Pfad (optional)", java),
    h("label", { class: "field" }, "Cosmetics-Quelle", url),
    h(
      "button",
      {
        class: "btn primary",
        onclick: async () => {
          const next = { ...s, username: username.value.trim(), ram_mb: Number(ram.value) || 4096, java_path: java.value.trim(), cosmetics_url: url.value.trim() };
          await call("save_settings", { settings: next });
          state.settings = next;
          renderNav();
          toast("Gespeichert.");
        },
      },
      "Speichern"
    )
  );
}

// ---------- boot ----------
(async () => {
  state.settings = await invoke("get_settings");
  render();
})();
