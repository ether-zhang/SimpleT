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
  let startup;
  let nextTimer = 1;
  let nextFrame = 1;

  function makeElement(selector = "") {
    const classes = new Set();
    const listeners = new Map();
    const attributes = new Map();
    return {
      id: selector.startsWith("#") ? selector.slice(1) : "",
      children: [], hidden: false, scrollTop: 0, scrollHeight: 0, clientHeight: 400, offsetTop: 0,
      value: "", textContent: "", disabled: false,
      classList: {
        add(...names) { names.forEach((name) => classes.add(name)); },
        remove(...names) { names.forEach((name) => classes.delete(name)); },
        toggle(name, enabled) { enabled ? classes.add(name) : classes.delete(name); },
        contains(name) { return classes.has(name); },
      },
      addEventListener(name, handler) { listeners.set(name, handler); },
      setAttribute(name, value) { attributes.set(name, value); },
      getAttribute(name) { return attributes.get(name); },
      removeAttribute(name) { attributes.delete(name); },
      dispatch(name, event = {}) {
        event.preventDefault ??= function () { this.defaultPrevented = true; };
        return listeners.get(name)?.(event);
      },
      appendChild(child) { this.children.push(child); child.parentElement = this; },
      replaceChildren(...children) {
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

  for (const id of ["lang-a", "lang-b", "cfg-ui-lang"]) {
    element(`#${id}-picker`).appendChild(element(`#${id}`));
    element(`#${id}-picker`).appendChild(element(`#${id}-menu`));
    element(`#${id}`).appendChild(element(`#${id}-label`));
  }

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
          if (command === "translate") return new Promise((resolve, reject) => {
            requests.push({ args, resolve, reject });
          });
          if (command === "load_history") return { entries: [], has_more: false, save_error: null };
          if (command === "frontend_ready") events.get("navigate")?.({ payload: {
            page: options.initialPage || "translate", origin: "bottom", generation: 1,
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
      querySelector: element, createElement: () => makeElement(), documentElement: {},
      addEventListener(name, handler) { documentListeners.set(name, handler); },
    },
    requestAnimationFrame(handler) { const id = nextFrame++; frames.set(id, handler); return id; },
    cancelAnimationFrame(id) { frames.delete(id); },
    setTimeout(handler) { const id = nextTimer++; timers.set(id, handler); return id; },
    clearTimeout(id) { timers.delete(id); },
    Intl,
  };
  vm.createContext(context);
  vm.runInContext(source, context);
  await startup();
  return {
    element, commands, requests, timers,
    emit(name, payload) { events.get(name)?.({ payload }); },
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
