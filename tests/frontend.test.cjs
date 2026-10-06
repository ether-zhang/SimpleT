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
  const commands = [];
  const requests = [];
  const timers = new Map();
  const events = new Map();
  const frames = new Map();
  let startup;
  let nextTimer = 1;
  let nextFrame = 1;

  function element(selector) {
    if (!elements.has(selector)) {
      const classes = new Set();
      const listeners = new Map();
      elements.set(selector, {
        value: "", textContent: "", disabled: false,
        classList: {
          add(...names) { names.forEach((name) => classes.add(name)); },
          remove(...names) { names.forEach((name) => classes.delete(name)); },
          toggle(name, enabled) { enabled ? classes.add(name) : classes.delete(name); },
          contains(name) { return classes.has(name); },
        },
        addEventListener(name, handler) { listeners.set(name, handler); },
        dispatch(name, event = {}) { return listeners.get(name)?.(event); },
        appendChild() {}, focus() { this.focused = true; }, offsetHeight: 0,
      });
    }
    return elements.get(selector);
  }

  const context = {
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
          if (command === "frontend_ready") events.get("navigate")?.({ payload: {
            page: options.initialPage || "translate", origin: "bottom", generation: 1,
          } });
          if (command === "request_hide") {
            events.get("flyout-hide")?.({ payload: { generation: args.generation } });
          }
        } },
        event: { listen: async (name, handler) => { events.set(name, handler); return () => {}; } },
      },
      addEventListener(name, handler) { if (name === "DOMContentLoaded") startup = handler; },
    },
    document: {
      querySelector: element, createElement: () => ({}), documentElement: {},
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
    keydown(event) { documentListeners.get("keydown")(event); },
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

test("an unchanged request updates the result and unlocks translation", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  const pending = app.translate();
  assert.equal(app.element("#translate-btn").disabled, true);
  assert.equal(app.requests[0].args.langA, "English");
  app.requests[0].resolve("你好");
  await pending;
  assert.equal(app.element("#output").value, "你好");
  assert.equal(app.element("#status").textContent, "");
  assert.equal(app.element("#translate-btn").disabled, false);
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

test("configuration read errors are visible and clear after explicit save", async () => {
  const app = await createApp({ load_error: "configuration is damaged" });
  assert.equal(app.element("#status").textContent, "configuration is damaged");
  assert.equal(app.element("#cfg-status").textContent, "configuration is damaged");
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.element("#status").textContent, "");
  assert.ok(app.commands.some(({ command }) => command === "save_config"));
});

test("saving settings does not clear an active translation's status", async () => {
  const app = await createApp();
  app.element("#input").value = "hello";
  const pending = app.translate();
  const progressMessage = app.element("#status").textContent;
  await app.element("#cfg-save").dispatch("click");
  assert.equal(app.element("#status").textContent, progressMessage);
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
  const autosave = app.element("#lang-a").dispatch("change");
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
