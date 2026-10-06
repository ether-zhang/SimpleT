const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { resolve } = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const source = readFileSync(resolve(__dirname, "../src/main.js"), "utf8");

// Execute the real frontend with a minimal DOM and a controllable Tauri bridge.
// Deferred requests let tests reproduce user actions while translation is pending.
async function createApp(config = {}) {
  const elements = new Map();
  const documentListeners = new Map();
  const commands = [];
  const requests = [];
  const timers = new Map();
  let startup;
  let nextTimer = 1;

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
        },
        addEventListener(name, handler) { listeners.set(name, handler); },
        dispatch(name, event = {}) { return listeners.get(name)?.(event); },
        appendChild() {}, focus() {}, offsetHeight: 0,
      });
    }
    return elements.get(selector);
  }

  const context = {
    window: {
      __TAURI__: {
        core: { invoke: async (command, args) => {
          commands.push({ command, args });
          if (command === "load_config") return {
            base_url: "https://example.com/v1", model: "test-model",
            lang_a: "English", lang_b: "Chinese", ui_lang: "zh",
            api_key_configured: true, ...config,
          };
          if (command === "translate") return new Promise((resolve, reject) => {
            requests.push({ args, resolve, reject });
          });
        } },
        event: { listen: async () => () => {} },
      },
      addEventListener(name, handler) { if (name === "DOMContentLoaded") startup = handler; },
    },
    document: {
      querySelector: element, createElement: () => ({}), documentElement: {},
      addEventListener(name, handler) { documentListeners.set(name, handler); },
    },
    requestAnimationFrame: () => 1, cancelAnimationFrame() {},
    setTimeout(handler) { const id = nextTimer++; timers.set(id, handler); return id; },
    clearTimeout(id) { timers.delete(id); },
    Intl,
  };
  vm.createContext(context);
  vm.runInContext(source, context);
  await startup();
  return {
    element, commands, requests, timers,
    keydown(event) { documentListeners.get("keydown")(event); },
    translate() { return element("#translate-btn").dispatch("click"); },
  };
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
