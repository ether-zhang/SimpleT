const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { resolve } = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const source = readFileSync(resolve(__dirname, "../src/main.js"), "utf8");

// Execute the real frontend with a minimal DOM and a controllable Tauri bridge.
// Deferred requests let tests reproduce user actions while translation is pending.
async function createApp(config = {}, options = {}) {
  const elements = new Map();
  const documentListeners = new Map();
  const windowListeners = new Map();
  const commands = [];
  const requests = [];
  const timers = new Map();
  const events = new Map();
  const frames = new Map();
  const mutations = [];
  const metrics = { createdNodes: 0, classUpdates: 0, styleReads: 0, rectangleReads: 0 };
  let startup;
  let nextTimer = 1;
  let nextFrame = 1;

  function makeElement(selector = "") {
    metrics.createdNodes++;
    const classes = new Set();
    const listeners = new Map();
    const attributes = new Map();
    const styles = new Map();
    return {
      selector,
      get firstElementChild() { return this.children[0] || null; },
      get nextElementSibling() {
        return this.parentElement?.children[this.parentElement.children.indexOf(this) + 1] || null;
      },
      id: selector.startsWith("#") ? selector.slice(1) : "",
      children: [], hidden: false, scrollTop: 0, scrollHeight: 0, clientHeight: 400, offsetTop: 0,
      value: "", textContent: "", disabled: false,
      style: {
        setProperty(name, value) { styles.set(name, value); },
        removeProperty(name) { styles.delete(name); },
        getPropertyValue(name) { return styles.get(name) || ""; },
      },
      getBoundingClientRect() {
        metrics.rectangleReads++;
        let hidden = false;
        for (let parent = this; parent; parent = parent.parentElement) {
          if (parent.hidden || parent.classList.contains("hidden")) hidden = true;
        }
        const rect = this.rect || options.rectangles?.[selector]
          || (selector === ".card" ? { x: 6, y: 6, width: 744, height: 471 } : {});
        const { x = 0, y = 0 } = rect;
        const width = hidden ? 0 : rect.width || 0;
        const height = hidden ? 0 : rect.height || 0;
        return { x, y, width, height, left: x, top: y, right: x + width, bottom: y + height };
      },
      classList: {
        add(...names) { metrics.classUpdates++; names.forEach((name) => classes.add(name)); },
        remove(...names) { metrics.classUpdates++; names.forEach((name) => classes.delete(name)); },
        toggle(name, enabled) { metrics.classUpdates++; enabled ? classes.add(name) : classes.delete(name); },
        contains(name) { return classes.has(name); },
      },
      addEventListener(name, handler) { listeners.set(name, handler); },
      setAttribute(name, value) { attributes.set(name, value); },
      getAttribute(name) { return attributes.get(name); },
      removeAttribute(name) { attributes.delete(name); },
      dispatch(name, event = {}) {
        event.target ??= this;
        event.preventDefault ??= function () { this.defaultPrevented = true; };
        return listeners.get(name)?.(event);
      },
      appendChild(child) { child.remove(); this.children.push(child); child.parentElement = this; return child; },
      insertBefore(child, before) {
        if (child === before) return child;
        child.remove();
        const index = before ? this.children.indexOf(before) : this.children.length;
        assert.ok(index >= 0);
        this.children.splice(index, 0, child);
        child.parentElement = this;
        return child;
      },
      remove() {
        if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(child => child !== this);
        this.parentElement = null;
      },
      replaceChildren(...children) {
        this.children.forEach(child => { child.parentElement = null; });
        this.children = [];
        children.forEach((child) => this.appendChild(child));
      },
      contains(target) {
        for (let node = target; node; node = node.parentElement) if (node === this) return true;
        return false;
      },
      setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; },
      focus() { this.focused = true; }, offsetHeight: 28,
    };
  }

  function element(selector) {
    if (!elements.has(selector)) elements.set(selector, makeElement(selector));
    return elements.get(selector);
  }

  for (const id of ["lang-a", "lang-b", "cfg-ui-lang", "cfg-appearance", "cfg-model"]) {
    element(`#${id}-picker`).appendChild(element(`#${id}`));
    element(`#${id}-picker`).appendChild(element(`#${id}-menu`));
    element(`#${id}`).appendChild(element(`#${id}-label`));
  }
  for (const id of ["cfg-model-search", "cfg-model-providers", "cfg-model-options", "cfg-model-empty"]) {
    element("#cfg-model-menu").appendChild(element(`#${id}`));
  }
  for (const [page, ids] of [
    ["translate", ["input", "output", "lang-a-picker", "lang-b-picker", "swap", "translate-btn", "open-settings", "open-history"]],
    ["settings", ["cfg-back", "cfg-url", "cfg-key", "cfg-model-picker", "cfg-model-status", "cfg-model-retry", "cfg-ui-lang-picker", "cfg-appearance-picker", "cfg-key-clear", "cfg-save"]],
    ["history", ["history-back", "history-scroll"]],
  ]) {
    element(".card").appendChild(element(`#page-${page}`));
    ids.forEach(id => element(`#page-${page}`).appendChild(element(`#${id}`)));
  }
  element("#history-scroll").appendChild(element("#history-list"));

  const context = {
    navigator: { platform: options.platform || "Win32" },
    window: {
      __TAURI__: {
        core: { invoke: async (command, args) => {
          commands.push({ command, args });
          if (options.invoke && Object.hasOwn(options.invoke, command)) {
            return options.invoke[command](args);
          }
          if (command === "load_config") return {
            base_url: "https://example.com/v1", model: "test-model",
            lang_a: "English", lang_b: "Chinese", ui_lang: "zh",
            api_key_configured: true, ...config,
          };
          if (command === "list_models") return [config.model || "test-model", "new-model"];
          if (command === "translate") return new Promise((resolve, reject) => {
            requests.push({ args, resolve, reject });
          });
          if (command === "load_history") return { entries: [], has_more: false, save_error: null };
          if (command === "frontend_ready") events.get("navigate")?.({ payload: {
            page: options.initialPage || "translate", origin: "bottom", generation: 1,
            nativeReveal: options.nativeReveal || false,
          } });
          if (command === "request_hide") {
            events.get("flyout-hide")?.({ payload: { generation: args.generation } });
          }
        } },
        event: { listen: async (name, handler) => { events.set(name, handler); return () => {}; } },
      },
      addEventListener(name, handler) {
        if (name === "DOMContentLoaded") startup = handler;
        else windowListeners.set(name, handler);
      },
    },
    document: {
      querySelector: element, createElement: () => makeElement(), documentElement: makeElement(),
      createElementNS: () => makeElement(),
      querySelectorAll() { return (options.surfaces || []).map(element); },
      addEventListener(name, handler) { documentListeners.set(name, handler); },
    },
    getComputedStyle(element) {
      metrics.styleReads++;
      return { borderTopLeftRadius: "18px", overflowX: "visible", overflowY: "visible", ...options.styles?.[element.selector] };
    },
    MutationObserver: class {
      constructor(callback) { mutations.push(callback); }
      observe() {}
    },
    requestAnimationFrame(handler) { const id = nextFrame++; frames.set(id, handler); return id; },
    cancelAnimationFrame(id) { frames.delete(id); },
    setTimeout(handler) { const id = nextTimer++; timers.set(id, handler); return id; },
    clearTimeout(id) { timers.delete(id); },
    Intl: options.intl || Intl,
    URL,
  };
  vm.createContext(context);
  vm.runInContext(source, context);
  await startup();
  return {
    element, commands, requests, timers, metrics, root: context.document.documentElement,
    frameCount: () => frames.size,
    emit(name, payload) { events.get(name)?.({ payload }); },
    mutateLayout() { mutations.forEach(callback => callback()); },
    runTimer(id) { const handler = timers.get(id); timers.delete(id); return handler?.(); },
    flushFrames() {
      const pending = [...frames.values()];
      frames.clear();
      pending.forEach((handler) => handler());
    },
    documentEvent(name, event) { return documentListeners.get(name)?.(event); },
    windowEvent(name) { return windowListeners.get(name)?.(); },
    keydown(event) {
      event.preventDefault ??= function () { this.defaultPrevented = true; };
      documentListeners.get("keydown")(event);
    },
    translate() { return element("#translate-btn").dispatch("click"); },
  };
}

function deferred() {
  let resolve;
  const promise = new Promise((complete) => { resolve = complete; });
  return { promise, resolve };
}

async function flushPromises() {
  await Promise.resolve();
  await Promise.resolve();
}

function historyEntry(id, text = `source ${id}`) {
  return {
    id, created_at: 1792252800000 + id * 60000, source_text: text,
    translation: `译文 ${id}`, lang_a: "English", lang_b: "Chinese",
  };
}

function historyIds(app) {
  return app.element("#history-list").children.map((child) => Number(child.getAttribute("data-history-id")));
}

function simulateHistoryHeight(app) {
  const scroll = app.element("#history-scroll");
  Object.defineProperty(scroll, "scrollHeight", { get: () =>
    app.element("#history-list").children.length * 100
      + (app.element("#history-more").classList.contains("hidden") ? 0 : 32),
  });
  scroll.clientHeight = 100;
  return scroll;
}

test("history opens at the newest bottom entry and returning preserves the translation draft", async () => {
  const app = await createApp({}, { invoke: {
    load_history: async () => ({ entries: [historyEntry(4), historyEntry(5)], has_more: true }),
  } });
  const scroll = simulateHistoryHeight(app);
  app.element("#input").value = "keep draft";
  app.element("#output").value = "keep translation";
  await app.element("#open-history").dispatch("click");
  assert.equal(app.element("#page-translate").classList.contains("hidden"), true);
  assert.equal(app.element("#page-settings").classList.contains("hidden"), true);
  assert.equal(app.element("#page-history").classList.contains("hidden"), false);
  assert.deepEqual(historyIds(app), [4, 5]);
  assert.equal(scroll.scrollTop, scroll.scrollHeight);
  assert.equal(app.element("#history-empty").classList.contains("hidden"), true);
  app.flushFrames();
  assert.equal(scroll.focused, true);
  app.element("#history-back").dispatch("click");
  assert.equal(app.element("#page-history").classList.contains("hidden"), true);
  assert.equal(app.element("#input").value, "keep draft");
  assert.equal(app.element("#output").value, "keep translation");
});

test("scrolling upward loads older entries once and preserves the reading position", async () => {
  const older = deferred();
  const app = await createApp({}, { invoke: {
    load_history: async ({ before }) => before ? older.promise
      : { entries: [historyEntry(4), historyEntry(5)], has_more: true },
  } });
  const scroll = simulateHistoryHeight(app);
  await app.element("#open-history").dispatch("click");
  scroll.scrollTop = 14;
  scroll.dispatch("scroll");
  scroll.dispatch("scroll");
  assert.equal(app.commands.filter((call) => call.command === "load_history").length, 2);
  assert.equal(app.commands.at(-1).args.before, 4);
  assert.equal(scroll.getAttribute("aria-busy"), "true");
  older.resolve({ entries: [historyEntry(2), historyEntry(3)], has_more: false });
  await new Promise(setImmediate);
  assert.deepEqual(historyIds(app), [2, 3, 4, 5]);
  assert.equal(scroll.scrollTop, 14 + 200 - 32);
  assert.equal(scroll.getAttribute("aria-busy"), "false");
  assert.equal(app.element("#history-more").classList.contains("hidden"), true);
});

test("an empty history has a localized empty state and does not request older pages", async () => {
  const app = await createApp({ ui_lang: "en" });
  await app.element("#open-history").dispatch("click");
  app.element("#history-scroll").dispatch("scroll");
  assert.equal(app.element("#open-history").getAttribute("aria-label"), "History");
  assert.equal(app.element("#history-title").textContent, "History");
  assert.equal(app.element("#history-empty").textContent, "Completed translations will appear here");
  assert.equal(app.element("#history-empty").classList.contains("hidden"), false);
  assert.equal(app.commands.filter((call) => call.command === "load_history").length, 1);
});

test("history read errors can be retried without showing a misleading empty state", async () => {
  let fail = true;
  const app = await createApp({}, { invoke: {
    load_history: async () => {
      if (fail) throw "history file unreadable";
      return { entries: [historyEntry(1)], has_more: false };
    },
  } });
  await app.element("#open-history").dispatch("click");
  assert.match(app.element("#history-status").textContent, /历史记录加载失败.*history file unreadable/);
  assert.equal(app.element("#history-empty").classList.contains("hidden"), true);
  assert.equal(app.element("#history-retry").classList.contains("hidden"), false);
  fail = false;
  await app.element("#history-retry").dispatch("click");
  assert.deepEqual(historyIds(app), [1]);
  assert.equal(app.element("#history-notice").classList.contains("hidden"), true);
});

test("a response from a previous history visit cannot replace the reopened page", async () => {
  const old = deferred();
  let count = 0;
  const app = await createApp({}, { invoke: {
    load_history: async () => ++count === 1 ? old.promise
      : { entries: [historyEntry(9)], has_more: false },
  } });
  const firstVisit = app.element("#open-history").dispatch("click");
  app.element("#history-back").dispatch("click");
  await app.element("#open-history").dispatch("click");
  old.resolve({ entries: [historyEntry(1)], has_more: true });
  await firstVisit;
  assert.deepEqual(historyIds(app), [9]);
  assert.equal(app.element("#history-scroll").getAttribute("aria-busy"), "false");
});

test("translations completing during initial history loading are merged without duplicates", async () => {
  const pending = deferred();
  const app = await createApp({}, { invoke: { load_history: () => pending.promise } });
  const loading = app.element("#open-history").dispatch("click");
  app.emit("history-updated", { entry: historyEntry(2) });
  app.emit("history-updated", { entry: historyEntry(3) });
  pending.resolve({ entries: [historyEntry(1), historyEntry(2)], has_more: false });
  await loading;
  assert.deepEqual(historyIds(app), [1, 2, 3]);
});

test("live history follows the bottom while leaving an older reading position alone", async () => {
  const app = await createApp({}, { invoke: {
    load_history: async () => ({ entries: [historyEntry(1), historyEntry(2)], has_more: false }),
  } });
  const scroll = simulateHistoryHeight(app);
  await app.element("#open-history").dispatch("click");
  scroll.scrollTop = scroll.scrollHeight - scroll.clientHeight;
  app.emit("history-updated", { entry: historyEntry(3) });
  assert.deepEqual(historyIds(app), [1, 2, 3]);
  assert.equal(scroll.scrollTop, scroll.scrollHeight);
  scroll.scrollTop = 12;
  app.emit("history-updated", { entry: historyEntry(4) });
  assert.deepEqual(historyIds(app), [1, 2, 3, 4]);
  assert.equal(scroll.scrollTop, 12);
});

test("saving history can fail without discarding the successful translation", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  const translating = app.translate();
  app.emit("history-updated", { error: "disk full" });
  app.requests[0].resolve("你好");
  await translating;
  assert.equal(app.element("#output").value, "你好");
  assert.match(app.element("#status").textContent, /未能保存.*disk full/);
  app.emit("history-updated", { entry: historyEntry(2) });
  assert.equal(app.element("#status").textContent, "");
});

test("a newer successful save cannot be replaced by an older history page warning", async () => {
  const pending = deferred();
  const app = await createApp({}, { invoke: { load_history: () => pending.promise } });
  app.emit("history-updated", { error: "old failure" });
  const loading = app.element("#open-history").dispatch("click");
  app.emit("history-updated", { entry: historyEntry(1) });
  pending.resolve({ entries: [], has_more: false, save_error: "old failure" });
  await loading;
  assert.equal(app.element("#history-notice").classList.contains("hidden"), true);
  assert.deepEqual(historyIds(app), [1]);
});

test("history text is rendered literally, including markup, line breaks, and right-to-left text", async () => {
  const entry = { ...historyEntry(1, "<img src=x onerror=alert(1)>\nالعربية"), lang_a: "Arabic" };
  const app = await createApp({}, { invoke: {
    load_history: async () => ({ entries: [entry], has_more: false }),
  } });
  await app.element("#open-history").dispatch("click");
  const item = app.element("#history-list").children[0];
  const source = item.children[1].children[0];
  assert.equal(source.textContent, entry.source_text);
  assert.equal(source.dir, "auto");
  assert.equal(source.lang, "ar");
  assert.equal(source.children.length, 0);
});

test("an unchanged request updates the result and unlocks translation", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  const pending = app.translate();
  assert.equal(app.element("#translate-btn").disabled, true);
  assert.equal(app.element("#translate-label").textContent, "翻译");
  assert.equal(app.element("#translate-hint").textContent, "翻译中…");
  assert.equal(app.element("#status").textContent, "");
  assert.equal(app.requests[0].args.langA, "English");
  app.requests[0].resolve("你好");
  await pending;
  assert.equal(app.element("#output").value, "你好");
  assert.equal(app.element("#status").textContent, "");
  assert.equal(app.element("#translate-btn").disabled, false);
  assert.equal(app.element("#translate-hint").textContent, "Ctrl + ↵");
});

for (const [platform, shortcut, modifier] of [["MacIntel", "⌘ + ↵", "metaKey"], ["Win32", "Ctrl + ↵", "ctrlKey"]]) {
  test(`the ${platform} shortcut hint matches keyboard submission`, async () => {
    const app = await createApp({}, { platform });
    assert.equal(app.element("#translate-hint").textContent, shortcut);
    app.element("#input").value = "hello";
    let prevented = false;
    app.element("#input").dispatch("keydown", {
      key: "Enter", [modifier]: true, preventDefault() { prevented = true; },
    });
    assert.equal(prevented, true);
    assert.equal(app.requests.length, 1);
    assert.equal(app.element("#translate-hint").textContent, "翻译中…");
    assert.equal(app.element("#status").textContent, "");
    app.requests[0].resolve("你好");
    await new Promise(setImmediate);
    assert.equal(app.element("#translate-hint").textContent, shortcut);
  });
}

test("changing the UI language updates an in-flight hint and restores the shortcut", async () => {
  const app = await createApp({}, { platform: "MacIntel" });
  app.element("#input").value = "hello";
  const pending = app.translate();
  app.element("#cfg-ui-lang").value = "en";
  await app.element("#cfg-ui-lang").dispatch("change");
  assert.equal(app.element("#translate-label").textContent, "Translate");
  assert.equal(app.element("#translate-hint").textContent, "Translating…");
  app.requests[0].resolve("你好");
  await pending;
  assert.equal(app.element("#translate-hint").textContent, "⌘ + ↵");
});

test("paste appends exact clipboard text instead of replacing the input selection", async () => {
  const app = await createApp({}, { invoke: {
    "plugin:clipboard-manager|read_text": async () => "\nworld",
  } });
  app.element("#input").value = "hello";
  app.element("#input").setSelectionRange(1, 4);
  app.element("#output").value = "keep result";
  await app.element("#paste-input").dispatch("click");
  assert.equal(app.element("#input").value, "hello\nworld");
  assert.equal(app.element("#input").selectionStart, 11);
  assert.equal(app.element("#input").selectionEnd, 11);
  assert.equal(app.element("#output").value, "keep result");
  assert.equal(app.element("#paste-input").disabled, false);
});

for (const content of [null, "", { format: "image" }]) {
  test(`paste silently ignores a non-text or empty clipboard: ${JSON.stringify(content)}`, async () => {
    const app = await createApp({}, { invoke: {
      "plugin:clipboard-manager|read_text": async () => content,
    } });
    app.element("#input").value = "keep input";
    app.element("#output").value = "keep result";
    await app.element("#paste-input").dispatch("click");
    assert.equal(app.element("#input").value, "keep input");
    assert.equal(app.element("#output").value, "keep result");
    assert.equal(app.element("#status").textContent, "");
    assert.equal(app.element("#paste-input").disabled, false);
    assert.notEqual(app.element("#input").focused, true);
  });
}

test("the native non-text clipboard error is ignored without hiding other errors", async () => {
  const app = await createApp({ load_error: "keep configuration warning" }, { invoke: {
    "plugin:clipboard-manager|read_text": async () => {
      throw "The clipboard contents were not available in the requested format or the clipboard is empty.";
    },
  } });
  app.element("#input").value = "keep input";
  await app.element("#paste-input").dispatch("click");
  assert.equal(app.element("#input").value, "keep input");
  assert.equal(app.element("#status").textContent, "keep configuration warning");
  assert.equal(app.element("#paste-input").disabled, false);
});

test("clearing input cancels a pending paste and duplicate paste clicks are ignored", async () => {
  const gate = deferred();
  const app = await createApp({}, { invoke: {
    "plugin:clipboard-manager|read_text": () => gate.promise,
  } });
  app.element("#input").value = "old input";
  const pending = app.element("#paste-input").dispatch("click");
  assert.equal(app.element("#paste-input").disabled, true);
  await app.element("#paste-input").dispatch("click");
  app.element("#clear-input").dispatch("click");
  gate.resolve("clipboard text");
  await pending;
  assert.equal(app.element("#input").value, "");
  assert.equal(app.commands.filter(({ command }) => command === "plugin:clipboard-manager|read_text").length, 1);
  assert.equal(app.element("#paste-input").disabled, false);
});

test("each clear action empties only its own pane and empty actions are disabled", async () => {
  const app = await createApp();
  for (const id of ["#clear-input", "#copy-output", "#clear-output"]) assert.equal(app.element(id).disabled, true);
  app.element("#input").value = "input";
  app.element("#output").value = "result";
  app.element("#input").dispatch("input");
  assert.equal(app.element("#clear-input").disabled, false);
  assert.equal(app.element("#copy-output").disabled, false);
  app.element("#clear-input").dispatch("click");
  assert.equal(app.element("#input").value, "");
  assert.equal(app.element("#output").value, "result");
  app.element("#input").value = "keep this";
  app.element("#clear-output").dispatch("click");
  assert.equal(app.element("#input").value, "keep this");
  assert.equal(app.element("#output").value, "");
  assert.equal(app.element("#copy-output").disabled, true);
  assert.equal(app.element("#clear-output").disabled, true);
});

test("copy writes the whole result and its success feedback follows the UI locale", async () => {
  const app = await createApp();
  app.element("#output").value = "  结果\n";
  await app.element("#copy-output").dispatch("click");
  const copied = app.commands.find(({ command }) => command === "plugin:clipboard-manager|write_text");
  assert.equal(copied.args.text, "  结果\n");
  assert.equal(app.element("#copy-output").classList.contains("copied"), true);
  assert.equal(app.element("#copy-output").getAttribute("aria-label"), "已复制");
  app.element("#cfg-ui-lang").value = "en";
  await app.element("#cfg-ui-lang").dispatch("change");
  assert.equal(app.element("#copy-output").title, "Copied");
  app.runTimer([...app.timers.keys()][0]);
  assert.equal(app.element("#copy-output").classList.contains("copied"), false);
  assert.equal(app.element("#copy-output").title, "Copy result");
  assert.equal(app.element("#status").textContent, "");
});

test("a copy finishing after the result was cleared cannot restore success feedback", async () => {
  const gate = deferred();
  const app = await createApp({}, { invoke: {
    "plugin:clipboard-manager|write_text": () => gate.promise,
  } });
  app.element("#output").value = "old result";
  const pending = app.element("#copy-output").dispatch("click");
  app.element("#clear-output").dispatch("click");
  gate.resolve();
  await pending;
  assert.equal(app.element("#output").value, "");
  assert.equal(app.element("#copy-output").classList.contains("copied"), false);
  assert.equal(app.element("#copy-output").disabled, true);
});

test("clipboard errors preserve the text and unlock the action buttons", async () => {
  let unavailable = true;
  const app = await createApp({}, { invoke: {
    "plugin:clipboard-manager|read_text": async () => { throw new Error("unavailable"); },
    "plugin:clipboard-manager|write_text": async () => {
      if (unavailable) throw new Error("unavailable");
    },
  } });
  app.element("#input").value = "input";
  app.element("#output").value = "result";
  await app.element("#paste-input").dispatch("click");
  assert.match(app.element("#status").textContent, /无法读取剪贴板/);
  await app.element("#copy-output").dispatch("click");
  assert.match(app.element("#status").textContent, /复制失败/);
  assert.equal(app.element("#input").value, "input");
  assert.equal(app.element("#output").value, "result");
  assert.equal(app.element("#paste-input").disabled, false);
  assert.equal(app.element("#copy-output").disabled, false);
  unavailable = false;
  await app.element("#copy-output").dispatch("click");
  assert.equal(app.element("#status").textContent, "");
  assert.equal(app.element("#copy-output").classList.contains("copied"), true);
});

for (const outcome of ["resolve", "reject"]) {
  test(`clearing the result discards a pending translation's ${outcome}`, async () => {
    const app = await createApp();
    app.element("#input").value = "hello";
    app.element("#output").value = "old result";
    const pending = app.translate();
    app.element("#clear-output").dispatch("click");
    app.requests[0][outcome]("late result");
    await pending;
    assert.equal(app.element("#input").value, "hello");
    assert.equal(app.element("#output").value, "");
    assert.equal(app.element("#status").textContent, "");
    assert.equal(app.element("#copy-output").disabled, true);
  });
}

test("clearing and retyping the same input still invalidates the older translation", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  app.element("#output").value = "keep result";
  const pending = app.translate();
  app.element("#clear-input").dispatch("click");
  app.element("#input").value = "hello";
  app.requests[0].resolve("late result");
  await pending;
  assert.equal(app.element("#output").value, "keep result");
});

test("only one language list opens and outside clicks dismiss it without changing the selection", async () => {
  const app = await createApp();
  app.element("#lang-a").dispatch("click");
  assert.equal(app.element("#lang-a-menu").hidden, false);
  assert.equal(app.element("#lang-a-menu").children.length, 13);
  assert.equal(app.element("#lang-a-menu").children[1].getAttribute("aria-selected"), "true");
  app.element("#lang-b").dispatch("click");
  assert.equal(app.element("#lang-a-menu").hidden, true);
  assert.equal(app.element("#lang-b-menu").hidden, false);
  app.documentEvent("pointerdown", { target: app.element("#input") });
  assert.equal(app.element("#lang-b-menu").hidden, true);
  app.element("#lang-a").dispatch("click");
  app.windowEvent("blur");
  assert.equal(app.element("#lang-a-menu").hidden, true);
  assert.equal(app.element("#lang-a").value, "English");
  assert.equal(app.commands.some(({ command }) => command === "save_languages"), false);
});

test("Escape cancels a highlighted language before it can close the flyout", async () => {
  const app = await createApp();
  app.element("#lang-a").dispatch("keydown", { key: "ArrowDown" });
  app.element("#lang-a").dispatch("keydown", { key: "ArrowDown" });
  assert.match(app.element("#lang-a").getAttribute("aria-activedescendant"), /-ja$/);
  assert.equal(app.element("#lang-a").value, "English");
  app.keydown({ key: "Escape" });
  assert.equal(app.element("#lang-a-menu").hidden, true);
  assert.equal(app.element("#lang-a").getAttribute("aria-expanded"), "false");
  assert.equal(app.timers.size, 0);
  assert.equal(app.commands.some(({ command }) => command === "save_languages"), false);
  app.keydown({ key: "Escape" });
  assert.equal(app.timers.size, 1);
});

test("keyboard selection persists the language and translation uses the chosen value", async () => {
  const app = await createApp();
  app.element("#lang-a").dispatch("keydown", { key: "End" });
  await app.element("#lang-a").dispatch("keydown", { key: "Enter" });
  assert.equal(app.element("#lang-a").value, "Vietnamese");
  assert.equal(app.element("#lang-a-menu").hidden, true);
  const saved = app.commands.find(({ command }) => command === "save_languages");
  assert.equal(saved.args.langA, "Vietnamese");
  assert.equal(saved.args.langB, "Chinese");
  app.element("#input").value = "xin chào";
  const pending = app.translate();
  assert.equal(app.requests[0].args.langA, "Vietnamese");
  app.requests[0].resolve("你好");
  await pending;
});

test("language typeahead works with localized names and Tab leaves the current value intact", async () => {
  const app = await createApp();
  app.element("#lang-a").dispatch("keydown", { key: "j", isComposing: true });
  assert.equal(app.element("#lang-a-menu").hidden, true);
  app.element("#lang-a").dispatch("keydown", { key: "j" });
  assert.match(app.element("#lang-a").getAttribute("aria-activedescendant"), /-ja$/);
  app.element("#lang-a").dispatch("keydown", { key: "Tab" });
  assert.equal(app.element("#lang-a-menu").hidden, true);
  assert.equal(app.element("#lang-a").value, "English");
  app.element("#lang-a").dispatch("keydown", { key: "j" });
  await app.element("#lang-a").dispatch("keydown", { key: "Enter" });
  assert.equal(app.element("#lang-a").value, "Japanese");
});

test("language labels and checkmarks follow swapping and UI locale changes", async () => {
  const app = await createApp();
  app.element("#lang-a").dispatch("click");
  await app.element("#lang-a-menu").children[4].dispatch("click");
  await app.element("#swap").dispatch("click");
  assert.equal(app.element("#lang-a").value, "Chinese");
  assert.equal(app.element("#lang-b").value, "French");
  app.element("#cfg-ui-lang").value = "en";
  await app.element("#cfg-ui-lang").dispatch("change");
  assert.match(app.element("#lang-a-label").textContent, /^Chinese/);
  assert.match(app.element("#lang-b-label").textContent, /^French/);
  assert.equal(app.element("#lang-a-menu").children[0].getAttribute("aria-selected"), "true");
  assert.equal(app.element("#lang-b-menu").children[4].getAttribute("aria-selected"), "true");
  assert.match(app.element("#lang-a").getAttribute("aria-label"), /^Source language:/);
});

test("the settings language picker restores its native name and saves only the UI language", async () => {
  const app = await createApp({ ui_lang: "ko" }, { initialPage: "settings" });
  assert.equal(app.element("#cfg-ui-lang-label").textContent, "한국어");
  app.element("#cfg-ui-lang").dispatch("click");
  const menu = app.element("#cfg-ui-lang-menu");
  assert.equal(menu.children.length, 8);
  assert.equal(menu.children[3].getAttribute("aria-selected"), "true");
  await menu.children[1].dispatch("click");
  assert.equal(app.element("#cfg-ui-lang").value, "en");
  assert.equal(app.element("#cfg-ui-lang-label").textContent, "English");
  assert.equal(app.element("#settings-title").textContent, "Settings");
  assert.equal(menu.hidden, true);
  assert.equal(menu.children[1].getAttribute("aria-selected"), "true");
  const saved = app.commands.find(({ command }) => command === "save_ui_lang");
  assert.equal(saved.args.uiLang, "en");
  assert.equal(app.commands.some(({ command }) => command === "save_languages"), false);
  assert.equal(app.element("#lang-a").value, "English");
  assert.equal(app.element("#lang-b").value, "Chinese");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.find(({ command }) => command === "save_config").args.config.ui_lang, "en");
});

test("settings language keyboard navigation uses its own choices and Escape cancels without saving", async () => {
  const app = await createApp({}, { initialPage: "settings" });
  const trigger = app.element("#cfg-ui-lang");
  trigger.dispatch("keydown", { key: "End" });
  assert.match(trigger.getAttribute("aria-activedescendant"), /-ru$/);
  app.keydown({ key: "Escape" });
  assert.equal(app.element("#cfg-ui-lang-menu").hidden, true);
  assert.equal(trigger.value, "zh");
  assert.equal(app.commands.some(({ command }) => command === "save_ui_lang"), false);
  assert.equal(app.commands.some(({ command }) => command === "request_hide"), false);
  trigger.dispatch("keydown", { key: "End" });
  await trigger.dispatch("keydown", { key: "Enter" });
  assert.equal(trigger.value, "ru");
  assert.equal(app.element("#cfg-ui-lang-label").textContent, "Русский");
  assert.equal(app.commands.find(({ command }) => command === "save_ui_lang").args.uiLang, "ru");
});

test("settings language typeahead searches native names and page navigation dismisses its list", async () => {
  const app = await createApp({}, { initialPage: "settings" });
  const trigger = app.element("#cfg-ui-lang");
  trigger.dispatch("keydown", { key: "d" });
  trigger.dispatch("keydown", { key: "e" });
  assert.match(trigger.getAttribute("aria-activedescendant"), /-de$/);
  trigger.dispatch("keydown", { key: "Tab" });
  assert.equal(trigger.value, "zh");
  assert.equal(app.commands.some(({ command }) => command === "save_ui_lang"), false);
  trigger.dispatch("click");
  app.element("#cfg-back").dispatch("click");
  assert.equal(app.element("#cfg-ui-lang-menu").hidden, true);
  app.element("#lang-a").dispatch("click");
  trigger.dispatch("click");
  assert.equal(app.element("#lang-a-menu").hidden, true);
  assert.equal(app.element("#cfg-ui-lang-menu").hidden, false);
  app.documentEvent("pointerdown", { target: app.element("#cfg-model") });
  assert.equal(app.element("#cfg-ui-lang-menu").hidden, true);
});

for (const [stored, expected] of [[undefined, "default"], ["unknown", "default"], ["liquid_glass", "liquid_glass"]]) {
  test(`appearance restores the saved choice with a safe default: ${stored}`, async () => {
    const app = await createApp({ appearance: stored }, { initialPage: "settings" });
    assert.equal(app.root.getAttribute("data-appearance"), expected);
    assert.equal(app.element("#cfg-appearance").value, expected);
    assert.equal(app.element("#cfg-appearance-menu").children.length, 2);
    assert.equal(app.element("#cfg-appearance-label").textContent, expected === "default" ? "默认" : "液态玻璃");
    assert.equal(app.commands.some(({ command }) => command === "save_appearance"), false);
  });
}

test("appearance applies immediately and persists without saving credential drafts", async () => {
  const gate = deferred();
  const app = await createApp({}, { initialPage: "settings", invoke: { save_appearance: () => gate.promise } });
  app.flushFrames();
  app.flushFrames();
  app.element("#cfg-key").value = "new key draft";
  app.element("#cfg-key").dispatch("input");
  app.element("#cfg-model").value = "new model draft";
  app.element("#cfg-appearance").dispatch("click");
  const saving = app.element("#cfg-appearance-menu").children[1].dispatch("click");
  assert.equal(app.root.getAttribute("data-appearance"), "liquid_glass");
  assert.equal(app.element("#cfg-appearance-menu").hidden, true);
  await flushPromises();
  assert.equal(app.commands.find(({ command }) => command === "save_appearance").args.appearance, "liquid_glass");
  const native = app.commands.find(({ command }) => command === "set_window_appearance");
  assert.equal(native.args.appearance, "liquid_glass");
  assert.equal(native.args.generation, 1);
  assert.equal(app.commands.some(({ command }) => command === "save_config"), false);
  assert.equal(app.commands.some(({ command }) => command === "save_ui_lang"), false);
  gate.resolve();
  await saving;
  assert.equal(app.element("#cfg-key").value, "new key draft");
  assert.equal(app.element("#cfg-model").value, "new model draft");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.find(({ command }) => command === "save_config").args.config.appearance, "liquid_glass");
});

test("appearance labels follow the UI locale and keyboard selection uses localized names", async () => {
  const app = await createApp({ appearance: "liquid_glass" }, { initialPage: "settings" });
  app.element("#cfg-ui-lang").dispatch("click");
  await app.element("#cfg-ui-lang-menu").children[1].dispatch("click");
  assert.equal(app.element("#cfg-appearance-label").textContent, "Liquid Glass");
  assert.equal(app.element("#lbl-appearance").textContent, "Appearance");
  assert.equal(app.root.getAttribute("data-appearance"), "liquid_glass");
  const picker = app.element("#cfg-appearance");
  picker.dispatch("keydown", { key: "d" });
  assert.match(picker.getAttribute("aria-activedescendant"), /-default$/);
  await picker.dispatch("keydown", { key: "Enter" });
  assert.equal(app.root.getAttribute("data-appearance"), "default");
  assert.equal(app.element("#cfg-appearance-label").textContent, "Default");
});

test("rapid appearance changes keep write order and ignore an obsolete save error", async () => {
  const gate = deferred();
  let saves = 0;
  const app = await createApp({}, { initialPage: "settings", invoke: {
    save_appearance: async () => {
      if (++saves === 1) { await gate.promise; throw new Error("old save failure"); }
    },
  } });
  const picker = app.element("#cfg-appearance");
  picker.value = "liquid_glass";
  const first = picker.dispatch("change");
  picker.value = "default";
  const second = picker.dispatch("change");
  await flushPromises();
  assert.equal(app.root.getAttribute("data-appearance"), "default");
  assert.equal(saves, 1);
  gate.resolve();
  await Promise.all([first, second]);
  assert.deepEqual(app.commands.filter(({ command }) => command === "save_appearance").map(call => call.args.appearance), ["liquid_glass", "default"]);
  assert.equal(app.element("#cfg-status").textContent, "");
});

test("a failed appearance save preserves the preview and a full save can recover it", async () => {
  const app = await createApp({}, { initialPage: "settings", invoke: {
    save_appearance: async () => { throw new Error("disk full"); },
  } });
  app.element("#cfg-appearance").value = "liquid_glass";
  await app.element("#cfg-appearance").dispatch("change");
  assert.match(app.element("#cfg-status").textContent, /外观设置失败.*disk full/);
  assert.equal(app.root.getAttribute("data-appearance"), "liquid_glass");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.find(({ command }) => command === "save_config").args.config.appearance, "liquid_glass");
  assert.match(app.element("#cfg-status").textContent, /已保存/);
});

test("prepared native glass reveals once, without changing material after an animation or dropdown", async () => {
  const app = await createApp({ appearance: "liquid_glass" }, { platform: "MacIntel", nativeReveal: true });
  const card = app.element(".card");
  assert.equal(app.root.getAttribute("data-native-reveal"), "true");
  assert.equal(app.root.getAttribute("data-native-backdrop"), "true");
  assert.equal(app.commands.some(({ command }) => command === "reveal_flyout"), false);
  app.flushFrames();
  assert.equal(app.commands.some(({ command }) => command === "reveal_flyout"), false);
  app.flushFrames();
  card.dispatch("transitionend", { propertyName: "opacity" });
  card.dispatch("transitionend", { propertyName: "clip-path" });
  app.element("#lang-a").dispatch("click");
  app.keydown({ key: "Escape" });
  assert.deepEqual(app.commands.filter(({ command }) => command === "reveal_flyout").map(call => call.args.generation), [1]);
  assert.equal(app.commands.some(({ command }) => command === "set_window_appearance"), false);
  await app.element("#close-flyout").dispatch("click");
  card.dispatch("transitionend", { propertyName: "clip-path" });
  app.emit("navigate", { page: "settings", generation: 2, origin: "top", nativeReveal: true });
  app.flushFrames();
  app.flushFrames();
  card.dispatch("transitionend", { propertyName: "clip-path" });
  assert.deepEqual(app.commands.filter(({ command }) => command === "reveal_flyout").map(call => call.args.generation), [1, 2]);
  assert.equal(app.commands.some(({ command }) => command === "set_window_appearance"), false);
  assert.equal(app.root.getAttribute("data-native-glass"), undefined);
  assert.equal(app.root.getAttribute("data-appearance"), "liquid_glass");
});

test("closing before the opening frames cancels native reveal, and a non-native reopen uses CSS", async () => {
  const app = await createApp({ appearance: "liquid_glass" }, { nativeReveal: true });
  app.flushFrames();
  await app.element("#close-flyout").dispatch("click");
  app.flushFrames();
  assert.equal(app.commands.some(({ command }) => command === "reveal_flyout"), false);
  app.emit("navigate", { page: "translate", generation: 2, origin: "top", nativeReveal: false });
  app.flushFrames();
  app.flushFrames();
  assert.equal(app.root.getAttribute("data-native-reveal"), "false");
  assert.equal(app.element(".card").classList.contains("show"), true);
  assert.equal(app.commands.some(({ command }) => command === "reveal_flyout"), false);
});

test("a prepared mask is released even if Default was chosen while navigation was pending", async () => {
  const app = await createApp({}, { nativeReveal: true });
  app.flushFrames();
  app.flushFrames();
  assert.equal(app.root.getAttribute("data-appearance"), "default");
  assert.deepEqual(app.commands.filter(({ command }) => command === "reveal_flyout").map(call => call.args.generation), [1]);
});

test("choosing Default before the opening frames still releases the prepared native mask", async () => {
  const app = await createApp({ appearance: "liquid_glass" }, { initialPage: "settings", nativeReveal: true });
  const picker = app.element("#cfg-appearance");
  picker.value = "default";
  await picker.dispatch("change");
  app.flushFrames();
  app.flushFrames();
  assert.equal(app.root.getAttribute("data-appearance"), "default");
  assert.deepEqual(app.commands.filter(({ command }) => command === "reveal_flyout").map(call => call.args.generation), [1]);
});

test("an explicit native appearance change keeps HTML fills and does not replay the opening animation", async () => {
  const nativeReady = deferred();
  const app = await createApp({}, { initialPage: "settings", surfaces: ["#cfg-url"],
    rectangles: { "#cfg-url": { x: 21, y: 100, width: 714, height: 36 } }, invoke: {
    set_window_appearance: () => nativeReady.promise,
  } });
  app.flushFrames();
  app.flushFrames();
  const picker = app.element("#cfg-appearance");
  picker.value = "liquid_glass";
  const change = picker.dispatch("change");
  assert.equal(app.root.getAttribute("data-appearance"), "liquid_glass");
  app.element("#cfg-url").rect = { x: 21, y: 112, width: 714, height: 36 };
  app.mutateLayout();
  app.flushFrames();
  nativeReady.resolve(true);
  await change;
  app.flushFrames();
  const updated = app.commands.find(call => call.command === "update_glass_surfaces");
  assert.equal(updated.args.surfaces[0].rect.y, 106);
  assert.equal(app.root.getAttribute("data-native-reveal"), "true");
  // Native masking never replaces the shared control fills or shadows.
  assert.equal(app.root.getAttribute("data-native-glass"), undefined);
  assert.equal(app.root.getAttribute("data-appearance"), "liquid_glass");
  assert.equal(app.commands.some(({ command }) => command === "reveal_flyout"), false);
});

test("a delayed native glass response cannot restore glass after choosing Default or closing", async () => {
  const pending = [];
  const app = await createApp({}, { initialPage: "settings", invoke: {
    set_window_appearance: async ({ appearance }) => {
      if (appearance === "default") return false;
      const gate = deferred();
      pending.push(gate);
      return gate.promise;
    },
  } });
  app.flushFrames();
  app.flushFrames();
  const picker = app.element("#cfg-appearance");
  picker.value = "liquid_glass";
  const first = picker.dispatch("change");
  picker.value = "default";
  await picker.dispatch("change");
  pending[0].resolve(true);
  await first;
  assert.equal(app.root.getAttribute("data-native-reveal"), "false");
  picker.value = "liquid_glass";
  const second = picker.dispatch("change");
  await app.element("#close-flyout").dispatch("click");
  pending[1].resolve(true);
  await second;
  assert.equal(app.root.getAttribute("data-native-reveal"), "false");
});

test("glass surfaces cut the background tint and reach native setup before the first reveal", async () => {
  const app = await createApp({ appearance: "liquid_glass" }, {
    platform: "MacIntel", nativeReveal: true, surfaces: ["#input", "#cfg-url", "#lang-a-menu"],
    rectangles: {
      "#input": { x: 21, y: 69, width: 301, height: 393 },
      "#cfg-url": { x: 21, y: 100, width: 714, height: 36 },
      "#lang-a-menu": { x: 21, y: 63, width: 301, height: 300 },
    },
  });
  app.flushFrames();
  assert.equal(app.commands.some(call => call.command === "reveal_flyout"), false);
  app.flushFrames();
  const reveal = app.commands.find(call => call.command === "reveal_flyout").args;
  assert.deepEqual(JSON.parse(JSON.stringify(reveal.surfaces)), [{
    rect: { x: 15, y: 63, width: 301, height: 393 },
    clip: { x: 15, y: 63, width: 301, height: 393 }, radius: 18,
  }]);
  const mask = app.element(".card").style.getPropertyValue("--glass-background-mask");
  assert.equal(mask, 'url("#glass-background-mask")');
  assert.equal(app.element("#glass-mask-base").getAttribute("width"), "744");
  assert.equal(app.element("#glass-mask-base").getAttribute("height"), "471");
  const hole = app.element("#glass-mask-holes").children[0];
  for (const [name, value] of Object.entries({ x: "15", y: "63", width: "301", height: "393", rx: "18", fill: "black" })) {
    assert.equal(hole.getAttribute(name), value);
  }
  app.element("#lang-a").dispatch("click");
  app.mutateLayout();
  app.flushFrames();
  let updates = app.commands.filter(call => call.command === "update_glass_surfaces");
  assert.equal(updates.at(-1).args.surfaces.length, 2);
  assert.ok(updates.at(-1).args.revision > reveal.layoutRevision);
  app.keydown({ key: "Escape" });
  app.mutateLayout();
  app.flushFrames();
  updates = app.commands.filter(call => call.command === "update_glass_surfaces");
  assert.equal(updates.at(-1).args.surfaces.length, 1);
});

test("scrolling clips native glass to the history viewport and skips unchanged geometry", async () => {
  const app = await createApp({ appearance: "liquid_glass" }, {
    initialPage: "history", nativeReveal: true, surfaces: ["#test-history-row"],
    rectangles: {
      "#history-scroll": { x: 21, y: 130, width: 714, height: 200 },
      "#test-history-row": { x: 21, y: 85, width: 714, height: 180 },
    },
    styles: { "#history-scroll": { overflowY: "auto" } },
  });
  app.element("#test-history-row").parentElement = app.element("#history-scroll");
  app.flushFrames();
  app.flushFrames();
  const reveal = app.commands.find(call => call.command === "reveal_flyout").args;
  assert.equal(reveal.surfaces[0].rect.y, 79);
  assert.equal(reveal.surfaces[0].clip.y, 124);
  assert.equal(reveal.surfaces[0].clip.height, 135);
  app.element("#test-history-row").rect = { x: 21, y: 50, width: 714, height: 180 };
  app.documentEvent("scroll");
  app.flushFrames();
  let updates = app.commands.filter(call => call.command === "update_glass_surfaces");
  assert.equal(updates.length, 1);
  assert.equal(updates[0].args.surfaces[0].clip.height, 100);
  app.documentEvent("scroll");
  app.flushFrames();
  assert.equal(app.commands.filter(call => call.command === "update_glass_surfaces").length, 1);
  app.mutateLayout();
  await app.element("#close-flyout").dispatch("click");
  app.flushFrames();
  assert.equal(app.commands.filter(call => call.command === "update_glass_surfaces").length, 1);
});

test("open language lists close when navigating to settings or hiding the flyout", async () => {
  const app = await createApp();
  app.element("#lang-a").dispatch("click");
  app.element("#open-settings").dispatch("click");
  assert.equal(app.element("#lang-a-menu").hidden, true);
  app.element("#cfg-back").dispatch("click");
  app.element("#lang-b").dispatch("click");
  app.emit("flyout-hide", { generation: 1 });
  assert.equal(app.element("#lang-b-menu").hidden, true);
});

test("swapping while a request is pending preserves the swapped content", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  app.element("#output").value = "older result";
  const pending = app.translate();
  await app.element("#swap").dispatch("click");
  app.requests[0].resolve("你好");
  await pending;
  assert.equal(app.element("#lang-a").value, "Chinese");
  assert.equal(app.element("#input").value, "older result");
  assert.equal(app.element("#output").value, "hello");
  assert.equal(app.element("#status").textContent, "");
  assert.equal(app.element("#translate-btn").disabled, false);
});

for (const changedField of ["#input", "#lang-a", "#lang-b"]) {
  test(`a pending result is discarded after changing ${changedField}`, async () => {
    const app = await createApp();
    app.element("#input").value = "hello";
    app.element("#output").value = "keep this result";
    const pending = app.translate();
    app.element(changedField).value = "changed";
    app.requests[0].resolve("outdated result");
    await pending;
    assert.equal(app.element("#output").value, "keep this result");
    assert.equal(app.element("#translate-btn").disabled, false);
    assert.equal(app.element("#translate-hint").textContent, "Ctrl + ↵");
  });
}

test("an outdated request error does not clear the current output", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  const pending = app.translate();
  app.element("#input").value = "new text";
  app.element("#output").value = "current output";
  app.requests[0].reject("old request failed");
  await pending;
  assert.equal(app.element("#output").value, "current output");
  assert.equal(app.element("#status").textContent, "");
});

test("a current request error is displayed and allows retry", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  const pending = app.translate();
  app.requests[0].reject("request failed");
  await pending;
  assert.equal(app.element("#status").textContent, "request failed");
  assert.equal(app.element("#translate-btn").disabled, false);
  assert.equal(app.element("#translate-hint").textContent, "Ctrl + ↵");
});

for (const composing of [{ isComposing: true }, { isComposing: false, keyCode: 229 }]) {
  test(`IME Escape does not close the flyout: ${JSON.stringify(composing)}`, async () => {
    const app = await createApp();
    app.keydown({ key: "Escape", ...composing });
    assert.equal(app.timers.size, 0);
  });

  test(`IME Enter does not submit: ${JSON.stringify(composing)}`, async () => {
    const app = await createApp();
    app.element("#input").value = "正在输入";
    app.element("#input").dispatch("keydown", {
      key: "Enter", ctrlKey: true, ...composing,
      preventDefault() { assert.fail("IME input must not be intercepted"); },
    });
    assert.equal(app.requests.length, 0);
  });
}

test("Escape outside composition still closes the flyout", async () => {
  const app = await createApp();
  app.keydown({ key: "Escape", isComposing: false });
  assert.equal(app.timers.size, 1);
});

for (const page of ["translate", "settings"]) {
  test(`the shared close button hides the ${page} page without quitting`, async () => {
    const app = await createApp({}, { initialPage: page });
    app.element("#close-flyout").dispatch("click");
    assert.equal(app.timers.size, 1);
    app.runTimer([...app.timers.keys()][0]);
    assert.ok(app.commands.some(({ command }) => command === "commit_hide"));
    assert.equal(app.commands.some(({ command }) => command === "quit"), false);
  });
}

test("the close button tooltip and accessible label follow the UI language", async () => {
  const app = await createApp({ ui_lang: "en" });
  assert.equal(app.element("#close-flyout").title, "Hide window");
  assert.equal(app.element("#close-flyout").getAttribute("aria-label"), "Hide window");
  app.element("#cfg-ui-lang").value = "zh";
  await app.element("#cfg-ui-lang").dispatch("change");
  assert.equal(app.element("#close-flyout").title, "收起窗口");
});

test("configuration read errors are visible and clear after explicit save", async () => {
  const app = await createApp({ load_error: "configuration is damaged" });
  assert.equal(app.element("#status").textContent, "configuration is damaged");
  assert.equal(app.element("#cfg-status").textContent, "configuration is damaged");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.element("#status").textContent, "");
  assert.ok(app.commands.some(({ command }) => command === "save_config"));
});

test("saving settings preserves the inline progress hint", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  const pending = app.translate();
  const progressMessage = app.element("#translate-hint").textContent;
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.element("#translate-hint").textContent, progressMessage);
  assert.equal(app.element("#status").textContent, "");
  app.requests[0].resolve("你好");
  await pending;
});

test("startup acknowledges readiness and opens the pending Settings page", async () => {
  const app = await createApp({}, { initialPage: "settings" });
  assert.ok(app.commands.some(({ command }) => command === "frontend_ready"));
  assert.equal(app.element("#page-settings").classList.contains("hidden"), false);
  app.flushFrames();
  assert.equal(app.element("#cfg-url").focused, true);
  assert.notEqual(app.element("#input").focused, true);
});

test("controls remain usable after a configuration load failure", async () => {
  const app = await createApp({}, { invoke: {
    load_config: async () => { throw new Error("load failed"); },
  } });
  assert.ok(app.commands.some(({ command }) => command === "frontend_ready"));
  assert.match(app.element("#status").textContent, /load failed/);
  app.element("#cfg-url").value = "https://example.com/v1";
  app.element("#cfg-url").dispatch("input");
  await app.element("#cfg-url").dispatch("change");
  app.element("#cfg-model").dispatch("click");
  app.element("#cfg-model-options").children[0].dispatch("click");
  assert.match(app.element("#cfg-status").textContent, /load failed/);
  await app.element("#cfg-save").dispatch("click");
  assert.ok(app.commands.some(({ command }) => command === "save_config"));
});

test("a hide event from an older opening cannot close the current flyout", async () => {
  const app = await createApp();
  app.emit("navigate", { page: "translate", origin: "bottom", generation: 2 });
  app.emit("flyout-hide", { generation: 1 });
  assert.equal(app.timers.size, 0);
});

test("reopening cancels the close timer and commits carry the opening generation", async () => {
  const app = await createApp();
  app.keydown({ key: "Escape" });
  const closeTimer = [...app.timers.keys()][0];
  app.emit("navigate", { page: "settings", origin: "bottom", generation: 2 });
  app.runTimer(closeTimer);
  assert.equal(app.commands.filter(({ command }) => command === "commit_hide").length, 0);
  app.keydown({ key: "Escape" });
  app.runTimer([...app.timers.keys()][0]);
  const commit = app.commands.find(({ command }) => command === "commit_hide");
  assert.equal(commit.args.generation, 2);
});

for (const initiallyEdited of [false, true]) {
  test(`save completion preserves a newer API key draft: initiallyEdited=${initiallyEdited}`, async () => {
    const gate = deferred();
    let saves = 0;
    const app = await createApp({}, { invoke: {
      save_config: () => ++saves === 1 ? gate.promise : undefined,
    } });
    if (initiallyEdited) {
      app.element("#cfg-key").value = "first-key";
      app.element("#cfg-key").dispatch("input");
    }
    const saving = app.element("#cfg-save").dispatch("click");
    await flushPromises();
    app.element("#cfg-key").value = "newer-key";
    app.element("#cfg-key").dispatch("input");
    gate.resolve();
    await saving;
    assert.equal(app.element("#cfg-key").value, "newer-key");
    assert.match(app.element("#cfg-status").textContent, /未保存/);
    await app.element("#cfg-save").dispatch("click");
    assert.equal(app.commands.filter(({ command }) => command === "save_config")[1].args.config.api_key, "newer-key");
  });
}

test("clearing a key during save remains a pending deletion", async () => {
  const gate = deferred();
  let saves = 0;
  const app = await createApp({}, { invoke: {
    save_config: () => ++saves === 1 ? gate.promise : undefined,
  } });
  app.element("#cfg-key").value = "first-key";
  app.element("#cfg-key").dispatch("input");
  const saving = app.element("#cfg-save").dispatch("click");
  await flushPromises();
  app.element("#cfg-key-clear").dispatch("click");
  gate.resolve();
  await saving;
  assert.match(app.element("#cfg-status").textContent, /未保存/);
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.filter(({ command }) => command === "save_config")[1].args.config.api_key, "");
});

test("edits to other settings made during save remain visibly unsaved", async () => {
  const gate = deferred();
  const app = await createApp({}, { invoke: { save_config: () => gate.promise } });
  const saving = app.element("#cfg-save").dispatch("click");
  app.element("#cfg-model").value = "new-model";
  gate.resolve();
  await saving;
  assert.equal(app.element("#cfg-model").value, "new-model");
  assert.match(app.element("#cfg-status").textContent, /未保存/);
});

test("a slow settings save cannot be submitted twice", async () => {
  const gate = deferred();
  const app = await createApp({}, { invoke: { save_config: () => gate.promise } });
  const first = app.element("#cfg-save").dispatch("click");
  const second = app.element("#cfg-save").dispatch("click");
  await flushPromises();
  const disabledDuringSave = app.element("#cfg-save").disabled;
  gate.resolve();
  await Promise.all([first, second]);
  assert.equal(disabledDuringSave, true);
  assert.equal(app.commands.filter(({ command }) => command === "save_config").length, 1);
  assert.equal(app.element("#cfg-save").disabled, false);
});

test("configuration writes keep their order across autosave and full saves", async () => {
  const gate = deferred();
  const app = await createApp({}, { invoke: { save_languages: () => gate.promise } });
  app.element("#lang-a").dispatch("click");
  const autosave = app.element("#lang-a-menu").children[2].dispatch("click");
  const fullSave = app.element("#cfg-save").dispatch("click");
  await flushPromises();
  const fullSaveStartedEarly = app.commands.some(({ command }) => command === "save_config");
  gate.resolve();
  await Promise.all([autosave, fullSave]);
  assert.equal(fullSaveStartedEarly, false);
});

test("a Saved status timer cannot erase a later autosave error", async () => {
  const app = await createApp({}, { invoke: {
    save_ui_lang: async () => { throw new Error("autosave failed"); },
  } });
  await app.element("#cfg-save").dispatch("click");
  const savedTimer = [...app.timers.keys()][0];
  app.element("#cfg-ui-lang").value = "en";
  await app.element("#cfg-ui-lang").dispatch("change");
  app.runTimer(savedTimer);
  assert.match(app.element("#cfg-status").textContent, /autosave failed/);
});

async function editModelUrl(app, url) {
  app.element("#cfg-url").value = url;
  app.element("#cfg-url").dispatch("input");
  await app.element("#cfg-url").dispatch("change");
}

test("model discovery does not delay startup and restores a listed saved model", async () => {
  const pending = deferred();
  const app = await createApp({ model: "provider/saved" }, {
    invoke: { list_models: () => pending.promise },
  });
  assert.ok(app.commands.some(({ command }) => command === "frontend_ready"));
  assert.equal(app.element("#cfg-model").disabled, true);
  assert.equal(app.element("#cfg-model-label").textContent, "provider/saved");
  assert.match(app.element("#cfg-model-status").textContent, /正在检查/);
  pending.resolve(["provider/first", "provider/saved"]);
  await flushPromises();
  assert.equal(app.element("#cfg-model").disabled, false);
  assert.equal(app.element("#cfg-model").value, "provider/saved");
  assert.match(app.element("#cfg-model-status").textContent, /服务可用.*2/);
  assert.equal(app.element("#cfg-model-options").children[1].getAttribute("aria-selected"), "true");
  assert.equal(app.element("#cfg-key").value, "");
});

for (const url of ["", "not a URL", "file:///tmp/models", "https://user:secret@example.com/v1", "https://example.com/v1?key=secret"]) {
  test(`an unusable model URL disables selection without a request: ${url}`, async () => {
    const app = await createApp({ base_url: url, model: "" });
    assert.equal(app.commands.some(({ command }) => command === "list_models"), false);
    assert.equal(app.element("#cfg-model").disabled, true);
    app.element("#cfg-model").dispatch("click");
    assert.equal(app.element("#cfg-model-menu").hidden, true);
    await app.element("#cfg-save").dispatch("click");
    assert.equal(app.commands.some(({ command }) => command === "save_config"), false);
  });
}

test("URL edits immediately invalidate choices and debounce discovery to the latest address", async () => {
  const app = await createApp();
  app.element("#cfg-model").dispatch("click");
  for (const url of ["https://first.example/v1", "https://latest.example/api/v1"]) {
    app.element("#cfg-url").value = url;
    app.element("#cfg-url").dispatch("input");
  }
  assert.equal(app.element("#cfg-model").value, "");
  assert.equal(app.element("#cfg-model").disabled, true);
  assert.equal(app.element("#cfg-model-menu").hidden, true);
  assert.equal(app.commands.filter(({ command }) => command === "list_models").length, 1);
  assert.equal(app.timers.size, 1);
  await app.runTimer([...app.timers.keys()][0]);
  const calls = app.commands.filter(({ command }) => command === "list_models");
  assert.equal(calls.length, 2);
  assert.equal(calls[1].args.baseUrl, "https://latest.example/api/v1");
  assert.equal(app.element("#cfg-model").disabled, false);
  assert.equal(app.commands.some(({ command }) => command === "save_config"), false);
});

for (const outcome of ["success", "failure"]) {
  test(`an obsolete catalog ${outcome} cannot overwrite the current endpoint`, async () => {
    const old = deferred();
    const app = await createApp({}, { invoke: { list_models: ({ baseUrl }) => {
      if (baseUrl.includes("old.example")) return old.promise;
      return [baseUrl.includes("new.example") ? "new/model" : "test-model"];
    } } });
    const oldRequest = editModelUrl(app, "https://old.example/v1");
    await editModelUrl(app, "https://new.example/v1");
    old.resolve(outcome === "success" ? ["old/model"] : Promise.reject({ code: "http", status: 401 }));
    await oldRequest;
    assert.equal(app.element("#cfg-model").disabled, false);
    assert.equal(app.element("#cfg-model-options").children[0].getAttribute("aria-label"), "new/model");
    assert.match(app.element("#cfg-model-status").textContent, /服务可用/);
  });
}

for (const [error, message] of [
  [{ code: "http", status: 401 }, /认证失败/],
  [{ code: "http", status: 403 }, /认证失败/],
  [{ code: "http", status: 404 }, /HTTP 404/],
  [{ code: "network" }, /无法连接/],
  [{ code: "timeout" }, /连接超时/],
  [{ code: "invalid_response" }, /有效的模型列表/],
  [{ code: "empty" }, /未提供可用模型/],
]) {
  test(`a model discovery failure disables the list and can be retried: ${JSON.stringify(error)}`, async () => {
    let fail = true;
    const app = await createApp({}, { invoke: { list_models: () => {
      if (fail) throw error;
      return ["test-model"];
    } } });
    assert.equal(app.element("#cfg-model").disabled, true);
    assert.equal(app.element("#cfg-model").value, "test-model");
    assert.match(app.element("#cfg-model-status").textContent, message);
    assert.equal(app.element("#cfg-model-retry").hidden, false);
    fail = false;
    await app.element("#cfg-model-retry").dispatch("click");
    assert.equal(app.element("#cfg-model").disabled, false);
    assert.equal(app.element("#cfg-model-retry").hidden, true);
  });
}

test("a missing saved model requires an explicit selection instead of silently changing models", async () => {
  const app = await createApp({}, { invoke: { list_models: () => ["replacement/model"] } });
  assert.equal(app.element("#cfg-model").value, "");
  assert.equal(app.element("#cfg-model-label").textContent, "请选择模型");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.some(({ command }) => command === "save_config"), false);
  app.element("#cfg-model").dispatch("click");
  app.element("#cfg-model-options").children[0].dispatch("click");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.find(({ command }) => command === "save_config").args.config.model, "replacement/model");
});

test("a large catalog searches full IDs, renders them literally, and saves the exact choice", async () => {
  const longId = `provider/${"long-model-".repeat(30)}<literal>:free`;
  const models = Array.from({ length: 650 }, (_, i) => `provider/model-${i}`);
  const app = await createApp({}, { invoke: { list_models: () => [...models, longId] } });
  app.element("#cfg-model").dispatch("click");
  const search = app.element("#cfg-model-search");
  assert.equal(search.focused, true);
  assert.equal(app.element("#cfg-model-options").children.length, 651);
  search.value = "LONG-model-";
  search.dispatch("input");
  const options = app.element("#cfg-model-options").children;
  assert.equal(options.length, 1);
  assert.equal(options[0].children[1].textContent, longId.slice("provider/".length));
  assert.equal(options[0].children[1].title, longId);
  assert.equal(options[0].children[1].children.length, 0);
  search.dispatch("keydown", { key: "Enter" });
  assert.equal(app.element("#cfg-model").value, longId);
  assert.equal(app.element("#cfg-model-label").title, longId);
  assert.equal(app.element("#cfg-model-menu").hidden, true);
  assert.equal(app.commands.some(({ command }) => command === "save_config"), false);
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.find(({ command }) => command === "save_config").args.config.model, longId);
});

test("model search respects IME and empty results, and Escape closes only the dropdown first", async () => {
  const app = await createApp();
  app.element("#cfg-model").dispatch("click");
  const search = app.element("#cfg-model-search");
  search.value = "no-such-model";
  search.dispatch("input");
  assert.equal(app.element("#cfg-model-empty").hidden, false);
  assert.equal(search.getAttribute("aria-activedescendant"), undefined);
  search.dispatch("keydown", { key: "Enter" });
  assert.equal(app.element("#cfg-model-menu").hidden, false);
  search.value = "new-model";
  search.dispatch("input");
  search.dispatch("keydown", { key: "Enter", isComposing: true });
  assert.equal(app.element("#cfg-model").value, "test-model");
  app.keydown({ key: "Escape" });
  assert.equal(app.element("#cfg-model-menu").hidden, true);
  assert.equal(app.commands.some(({ command }) => command === "request_hide"), false);
  app.keydown({ key: "Escape" });
  assert.equal(app.commands.some(({ command }) => command === "request_hide"), true);
});

test("API key edits refresh discovery with the draft and clearing never reuses a saved key", async () => {
  const app = await createApp();
  app.element("#cfg-key").value = "draft-key";
  app.element("#cfg-key").dispatch("input");
  assert.equal(app.element("#cfg-model").disabled, true);
  await app.element("#cfg-key").dispatch("change");
  assert.equal(app.commands.filter(({ command }) => command === "list_models").at(-1).args.apiKey, "draft-key");
  await app.element("#cfg-key-clear").dispatch("click");
  await flushPromises();
  assert.equal(app.commands.filter(({ command }) => command === "list_models").at(-1).args.apiKey, "");
  assert.equal(app.element("#cfg-key").value, "");
});

test("moving to another public service does not carry the previous provider's saved key into a full save", async () => {
  const app = await createApp();
  await editModelUrl(app, "https://other.example/api/v1");
  assert.equal(app.element("#cfg-key").placeholder, "sk-…");
  app.element("#cfg-model").dispatch("click");
  app.element("#cfg-model-options").children[0].dispatch("click");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.find(({ command }) => command === "save_config").args.config.api_key, "");
});

test("model messages follow the UI locale without discarding the selection or refetching", async () => {
  const app = await createApp();
  app.element("#cfg-ui-lang").value = "en";
  await app.element("#cfg-ui-lang").dispatch("change");
  assert.match(app.element("#cfg-model-status").textContent, /Service available/);
  assert.equal(app.element("#cfg-model-search").placeholder, "Search models…");
  assert.equal(app.element("#cfg-model").value, "test-model");
  assert.equal(app.commands.filter(({ command }) => command === "list_models").length, 1);
});

test("reopening model search with End navigates the whole catalog after filtering", async () => {
  const app = await createApp();
  app.element("#cfg-model").dispatch("click");
  app.element("#cfg-model-search").value = "test-model";
  app.element("#cfg-model-search").dispatch("input");
  app.keydown({ key: "Escape" });
  app.element("#cfg-model").dispatch("keydown", { key: "End" });
  const options = app.element("#cfg-model-options").children;
  assert.equal(options.length, 2);
  assert.equal(app.element("#cfg-model-search").getAttribute("aria-activedescendant"), options[1].id);
  app.element("#cfg-model-search").dispatch("keydown", { key: "Enter" });
  assert.equal(app.element("#cfg-model").value, "new-model");
});

function modelProviderOption(app, provider) {
  return app.element("#cfg-model-providers").children.find((option) => option.getAttribute("data-provider") === provider);
}

function visibleModelIds(app) {
  return app.element("#cfg-model-options").children.map((option) => option.getAttribute("aria-label"));
}

test("the model cascader shows providers on the left and commits only a model chosen on the right", async () => {
  const app = await createApp({ model: "openai/gpt-a" }, { invoke: {
    list_models: () => ["openai/gpt-a", "anthropic/claude-a", "anthropic/claude-b", "google/gemini-a"],
  } });
  app.element("#cfg-model").dispatch("click");
  assert.equal(app.element("#cfg-model-providers").children.length, 3);
  assert.equal(modelProviderOption(app, "openai").children[0].textContent, "OpenAI");
  assert.equal(modelProviderOption(app, "openai").getAttribute("aria-selected"), "true");
  assert.deepEqual(visibleModelIds(app), ["openai/gpt-a"]);
  modelProviderOption(app, "anthropic").dispatch("click");
  assert.deepEqual(visibleModelIds(app), ["anthropic/claude-a", "anthropic/claude-b"]);
  assert.equal(app.element("#cfg-model-options").children[0].children[1].textContent, "claude-a");
  assert.equal(app.element("#cfg-model").value, "openai/gpt-a");
  assert.equal(app.element("#cfg-model-menu").hidden, false);
  assert.equal(app.commands.some(({ command }) => command === "save_config"), false);
  app.element("#cfg-model-options").children[1].dispatch("click");
  assert.equal(app.element("#cfg-model").value, "anthropic/claude-b");
  assert.equal(app.element("#cfg-model-menu").hidden, true);
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.find(({ command }) => command === "save_config").args.config.model, "anthropic/claude-b");
});

test("cancelling provider browsing preserves the chosen model and reopening restores its provider", async () => {
  const app = await createApp({ model: "openai/gpt-a" }, { invoke: {
    list_models: () => ["anthropic/claude-a", "openai/gpt-a"],
  } });
  app.element("#cfg-model").dispatch("click");
  modelProviderOption(app, "anthropic").dispatch("click");
  app.documentEvent("pointerdown", { target: app.element("#cfg-url") });
  assert.equal(app.element("#cfg-model").value, "openai/gpt-a");
  app.element("#cfg-model").dispatch("click");
  assert.equal(modelProviderOption(app, "openai").getAttribute("aria-selected"), "true");
  assert.deepEqual(visibleModelIds(app), ["openai/gpt-a"]);
  assert.equal(app.element("#cfg-model-options").children[0].getAttribute("aria-selected"), "true");
});

test("model search finds other providers and filters both columns without altering the saved choice", async () => {
  const app = await createApp({ model: "openai/gpt-a" }, { invoke: {
    list_models: () => ["openai/gpt-a", "anthropic/claude-a", "anthropic/claude-b", "x-ai/grok-a"],
  } });
  app.element("#cfg-model").dispatch("click");
  const search = app.element("#cfg-model-search");
  search.value = "CLAUDE";
  search.dispatch("input");
  assert.equal(app.element("#cfg-model-providers").children.length, 1);
  assert.equal(modelProviderOption(app, "anthropic").getAttribute("aria-selected"), "true");
  assert.deepEqual(visibleModelIds(app), ["anthropic/claude-a", "anthropic/claude-b"]);
  search.value = "xAI";
  search.dispatch("input");
  assert.deepEqual(visibleModelIds(app), ["x-ai/grok-a"]);
  search.value = "not-a-model";
  search.dispatch("input");
  assert.equal(app.element("#cfg-model-providers").children.length, 0);
  assert.equal(app.element("#cfg-model-empty").hidden, false);
  assert.equal(app.element("#cfg-model-providers").getAttribute("aria-activedescendant"), undefined);
  assert.equal(app.element("#cfg-model").value, "openai/gpt-a");
  search.value = "";
  search.dispatch("input");
  assert.equal(app.element("#cfg-model-providers").children.length, 3);
  assert.deepEqual(visibleModelIds(app), ["openai/gpt-a"]);
});

test("unknown provider names and nested model paths display literally and retain their full IDs", async () => {
  const id = "research-lab/release/model:v2";
  const app = await createApp({ model: id }, { invoke: {
    list_models: () => [id, "__proto__/literal", "<provider>/literal"],
  } });
  app.element("#cfg-model").dispatch("click");
  assert.equal(modelProviderOption(app, "research-lab").children[0].textContent, "research-lab");
  assert.equal(app.element("#cfg-model-options").children[0].children[1].textContent, "release/model:v2");
  assert.equal(app.element("#cfg-model-options").children[0].children[1].title, id);
  assert.equal(modelProviderOption(app, "<provider>").children[0].textContent, "<provider>");
  assert.equal(modelProviderOption(app, "<provider>").children[0].children.length, 0);
  modelProviderOption(app, "__proto__").dispatch("click");
  assert.deepEqual(visibleModelIds(app), ["__proto__/literal"]);
  app.element("#cfg-model-options").children[0].dispatch("click");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.commands.find(({ command }) => command === "save_config").args.config.model, "__proto__/literal");
});

test("models without a provider prefix share a localized Current service group", async () => {
  const app = await createApp({ model: "gpt-4.1" }, { invoke: {
    list_models: () => ["gpt-4.1", "llama3:latest", "other/model"],
  } });
  app.element("#cfg-model").dispatch("click");
  assert.equal(modelProviderOption(app, "").children[0].textContent, "当前服务");
  assert.deepEqual(visibleModelIds(app), ["gpt-4.1", "llama3:latest"]);
  app.element("#cfg-ui-lang").value = "en";
  await app.element("#cfg-ui-lang").dispatch("change");
  app.element("#cfg-model").dispatch("click");
  assert.equal(modelProviderOption(app, "").children[0].textContent, "Current service");
  assert.equal(app.element("#cfg-model-providers-title").textContent, "Providers");
  assert.equal(app.element("#cfg-model-models-title").textContent, "Models");
  assert.equal(app.element("#cfg-model").value, "gpt-4.1");
  assert.equal(app.commands.filter(({ command }) => command === "list_models").length, 1);
});

test("keyboard users can traverse provider and model columns without committing the provider", async () => {
  const app = await createApp({ model: "openai/gpt-a" }, { invoke: {
    list_models: () => ["anthropic/claude-a", "anthropic/claude-b", "openai/gpt-a"],
  } });
  app.element("#cfg-model").dispatch("click");
  const providers = app.element("#cfg-model-providers");
  const models = app.element("#cfg-model-options");
  app.element("#cfg-model-search").dispatch("keydown", { key: "Tab" });
  assert.equal(providers.focused, true);
  providers.dispatch("keydown", { key: "Home", isComposing: true });
  assert.deepEqual(visibleModelIds(app), ["openai/gpt-a"]);
  providers.dispatch("keydown", { key: "Home" });
  assert.deepEqual(visibleModelIds(app), ["anthropic/claude-a", "anthropic/claude-b"]);
  assert.equal(app.element("#cfg-model").value, "openai/gpt-a");
  providers.dispatch("keydown", { key: "Enter" });
  assert.equal(models.focused, true);
  assert.equal(app.element("#cfg-model-menu").hidden, false);
  models.dispatch("keydown", { key: "ArrowDown" });
  assert.equal(models.getAttribute("aria-activedescendant"), models.children[1].id);
  models.dispatch("keydown", { key: "ArrowLeft" });
  providers.dispatch("keydown", { key: "Tab" });
  models.dispatch("keydown", { key: "Enter" });
  assert.equal(app.element("#cfg-model").value, "anthropic/claude-b");
  assert.equal(app.element("#cfg-model-menu").hidden, true);
});

test("large catalogs expose only one provider's models at a time while searching across all groups", async () => {
  const catalog = Array.from({ length: 20 }, (_, provider) =>
    Array.from({ length: 50 }, (_, model) => `vendor-${String(provider).padStart(2, "0")}/model-${model}`)).flat();
  const app = await createApp({ model: catalog[0] }, { invoke: { list_models: () => catalog } });
  app.element("#cfg-model").dispatch("click");
  assert.equal(app.element("#cfg-model-providers").children.length, 20);
  assert.equal(visibleModelIds(app).length, 50);
  modelProviderOption(app, "vendor-19").dispatch("click");
  assert.equal(visibleModelIds(app).every((id) => id.startsWith("vendor-19/")), true);
  app.element("#cfg-model-search").value = "model-49";
  app.element("#cfg-model-search").dispatch("input");
  assert.equal(app.element("#cfg-model-providers").children.length, 20);
  assert.deepEqual(visibleModelIds(app), ["vendor-19/model-49"]);
  app.element("#cfg-model-options").children[0].dispatch("click");
  assert.equal(app.element("#cfg-model").value, "vendor-19/model-49");
});

test("paging history retains existing rows and creates only the newly loaded rows", async () => {
  const app = await createApp({}, { invoke: {
    load_history: async ({ before }) => ({
      entries: before ? [historyEntry(2), historyEntry(3)] : [historyEntry(4), historyEntry(5)],
      has_more: !before,
    }),
  } });
  const scroll = simulateHistoryHeight(app);
  await app.element("#open-history").dispatch("click");
  const previousRows = [...app.element("#history-list").children];
  const nodesBefore = app.metrics.createdNodes;
  scroll.scrollTop = 0;
  scroll.dispatch("scroll");
  await new Promise(setImmediate);
  const rows = app.element("#history-list").children;
  assert.deepEqual(historyIds(app), [2, 3, 4, 5]);
  assert.equal(rows[2], previousRows[0]);
  assert.equal(rows[3], previousRows[1]);
  assert.ok(app.metrics.createdNodes - nodesBefore <= 14, "only two new seven-node records are created");
  app.emit("history-updated", { entry: historyEntry(5, "updated literal <text>") });
  assert.equal(app.element("#history-list").children[3], previousRows[1]);
  assert.equal(previousRows[1].children[1].children[0].textContent, "updated literal <text>");
});

test("leaving history releases its page cache and revisiting reloads every retained record", async () => {
  let entries = [historyEntry(1)];
  const app = await createApp({}, { invoke: {
    load_history: async () => ({ entries, has_more: false }),
  } });
  app.element("#input").value = "unsent draft";
  app.element("#output").value = "existing result";
  await app.element("#open-history").dispatch("click");
  app.element("#history-back").dispatch("click");
  assert.equal(app.element("#history-list").children.length, 0);
  assert.equal(app.element("#input").value, "unsent draft");
  assert.equal(app.element("#output").value, "existing result");
  entries = [...entries, historyEntry(2)];
  app.emit("history-updated", { entry: entries[1] });
  assert.equal(app.element("#history-list").children.length, 0);
  await app.element("#open-history").dispatch("click");
  assert.deepEqual(historyIds(app), [1, 2]);
});

test("hundreds of glass layout updates reuse one mask resource and its rounded nodes", async () => {
  const app = await createApp({ appearance: "liquid_glass" }, {
    initialPage: "history", nativeReveal: true, surfaces: ["#test-history-row"],
    rectangles: {
      "#history-scroll": { x: 21, y: 130, width: 714, height: 200 },
      "#test-history-row": { x: 21, y: 85, width: 714, height: 180 },
    }, styles: { "#history-scroll": { overflowY: "auto" } },
  });
  app.element("#test-history-row").parentElement = app.element("#history-scroll");
  app.flushFrames();
  app.flushFrames();
  const hole = app.element("#glass-mask-holes").children[0];
  const clip = app.element("#glass-mask-clips").children[0];
  const createdBefore = app.metrics.createdNodes;
  const resources = new Set();
  for (let step = 0; step < 300; step++) {
    app.element("#test-history-row").rect = { x: 21, y: 85 - step / 10, width: 714, height: 180 };
    app.documentEvent("scroll");
    app.flushFrames();
    resources.add(app.element(".card").style.getPropertyValue("--glass-background-mask"));
    assert.equal(app.element("#glass-mask-holes").children[0], hole);
    assert.equal(app.element("#glass-mask-clips").children[0], clip);
  }
  assert.deepEqual([...resources], ['url("#glass-background-mask")']);
  assert.equal(app.metrics.createdNodes, createdBefore);
  assert.equal(hole.getAttribute("rx"), "18");
  app.element("#cfg-appearance").value = "default";
  await app.element("#cfg-appearance").dispatch("change");
  assert.equal(app.element("#glass-mask-holes").children.length, 0);
  assert.equal(app.element("#glass-mask-clips").children.length, 0);
});

test("one glass layout pass reads a shared clipping ancestor only once", async () => {
  const surfaces = Array.from({ length: 10 }, (_, index) => `#row-${index}`);
  const rectangles = { "#history-scroll": { x: 21, y: 100, width: 714, height: 300 } };
  surfaces.forEach((selector, index) => { rectangles[selector] = { x: 21, y: 130 + index * 20, width: 300, height: 20 }; });
  const app = await createApp({ appearance: "liquid_glass" }, {
    initialPage: "history", nativeReveal: true, surfaces, rectangles,
    styles: { "#history-scroll": { overflowY: "auto" } },
  });
  surfaces.forEach(selector => { app.element(selector).parentElement = app.element("#history-scroll"); });
  app.flushFrames();
  app.flushFrames();
  const before = app.metrics.styleReads;
  app.element(surfaces[0]).rect = { ...rectangles[surfaces[0]], y: 131 };
  app.documentEvent("scroll");
  app.flushFrames();
  assert.ok(app.metrics.styleReads - before < 20);
  assert.equal(app.commands.filter(call => call.command === "update_glass_surfaces").at(-1).args.surfaces.length, 10);
});

test("a closed flyout does not schedule glass layout frames for background mutations", async () => {
  const app = await createApp({ appearance: "liquid_glass" }, { nativeReveal: true });
  app.flushFrames();
  app.flushFrames();
  await app.element("#close-flyout").dispatch("click");
  app.flushFrames();
  for (let i = 0; i < 100; i++) app.mutateLayout();
  assert.equal(app.frameCount(), 0);
});

test("pointer movement in a large model group updates only the active option", async () => {
  const app = await createApp({ model: "provider/model-0" }, { invoke: {
    list_models: () => Array.from({ length: 650 }, (_, i) => `provider/model-${i}`),
  } });
  await flushPromises();
  app.element("#open-settings").dispatch("click");
  app.element("#cfg-model").dispatch("click");
  const options = app.element("#cfg-model-options").children;
  const before = app.metrics.classUpdates;
  for (let i = 0; i < 100; i++) options[50].dispatch("pointermove");
  assert.ok(app.metrics.classUpdates - before <= 2);
  assert.equal(options.filter(option => option.classList.contains("is-active")).length, 1);
  assert.equal(options[50].classList.contains("is-active"), true);
  assert.equal(app.element("#cfg-model").value, "provider/model-0");
});

test("history updates reuse language-name formatters while locale changes keep labels correct", async () => {
  const count = { dates: 0, names: 0 };
  const countedIntl = Object.create(Intl);
  countedIntl.DisplayNames = class {
    constructor(...args) { count.names++; return new Intl.DisplayNames(...args); }
  };
  countedIntl.DateTimeFormat = class {
    constructor(...args) { count.dates++; return new Intl.DateTimeFormat(...args); }
  };
  const app = await createApp({}, { intl: countedIntl, invoke: {
    load_history: async () => ({ entries: [historyEntry(1)], has_more: false }),
  } });
  assert.equal(count.dates, 0);
  const initialNames = count.names;
  await app.element("#open-history").dispatch("click");
  for (let id = 2; id <= 20; id++) app.emit("history-updated", { entry: historyEntry(id) });
  assert.equal(count.dates, 20);
  assert.equal(count.names, initialNames);
  const row = app.element("#history-list").children[0];
  assert.equal(row.children[0].children[0].textContent, "英语 → 中文");
  app.element("#cfg-ui-lang").value = "en";
  await app.element("#cfg-ui-lang").dispatch("change");
  assert.equal(count.dates, 21);
  assert.equal(app.element("#history-list").children[0], row);
  assert.equal(row.children[0].children[0].textContent, "English → Chinese");
  assert.equal(row.children[1].children[0].textContent, "source 1");
});

test("retained history rows follow system time-zone changes on subsequent renders", async () => {
  let zone = "UTC";
  const systemIntl = Object.create(Intl);
  systemIntl.DateTimeFormat = class {
    constructor(locales, options) { return new Intl.DateTimeFormat(locales, { ...options, timeZone: zone }); }
  };
  const entry = historyEntry(1);
  const app = await createApp({}, { intl: systemIntl, invoke: {
    load_history: async () => ({ entries: [entry], has_more: false }),
  } });
  await app.element("#open-history").dispatch("click");
  const row = app.element("#history-list").children[0];
  const time = row.children[0].children[1];
  const previous = time.textContent;
  zone = "Asia/Tokyo";
  app.emit("history-updated", { entry: historyEntry(2) });
  assert.equal(app.element("#history-list").children[0], row);
  assert.notEqual(time.textContent, previous);
  assert.equal(time.textContent, new Intl.DateTimeFormat("zh", {
    year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", timeZone: zone,
  }).format(new Date(entry.created_at)));
});
