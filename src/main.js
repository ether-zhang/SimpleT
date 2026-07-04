const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// value 会被写入 system prompt，用模型能理解的英文语言名；label 为界面显示
const LANGUAGES = [
  { value: "Chinese", label: "中文 (Chinese)" },
  { value: "English", label: "英语 (English)" },
  { value: "Japanese", label: "日语 (Japanese)" },
  { value: "Korean", label: "韩语 (Korean)" },
  { value: "French", label: "法语 (French)" },
  { value: "German", label: "德语 (German)" },
  { value: "Spanish", label: "西班牙语 (Spanish)" },
  { value: "Russian", label: "俄语 (Russian)" },
  { value: "Italian", label: "意大利语 (Italian)" },
  { value: "Portuguese", label: "葡萄牙语 (Portuguese)" },
  { value: "Arabic", label: "阿拉伯语 (Arabic)" },
  { value: "Thai", label: "泰语 (Thai)" },
  { value: "Vietnamese", label: "越南语 (Vietnamese)" },
];

let els = {};

function fillLangSelect(sel) {
  sel.innerHTML = "";
  for (const l of LANGUAGES) {
    const o = document.createElement("option");
    o.value = l.value;
    o.textContent = l.label;
    sel.appendChild(o);
  }
}

// 收起动画时长，需与 styles.css 中 .card 的 transition 时长保持一致
const CLOSE_MS = 280;
let hideTimer = null;

// 卡片从下方滑入（打开）
function slideIn() {
  clearTimeout(hideTimer);
  hideTimer = null;
  els.card.classList.remove("show");
  // 双 rAF：先让“隐藏态”绘制一帧，再触发过渡，避免直接闪现
  requestAnimationFrame(() =>
    requestAnimationFrame(() => els.card.classList.add("show"))
  );
}

// 卡片向下滑出（收起），动画结束后再真正隐藏窗口——这样能看到下滑过程
function slideOutThenHide() {
  if (hideTimer) return; // 已在收起中
  els.card.classList.remove("show");
  hideTimer = setTimeout(() => {
    hideTimer = null;
    invoke("commit_hide");
  }, CLOSE_MS + 40);
}

function showPage(page) {
  const isSettings = page === "settings";
  els.pageTranslate.classList.toggle("hidden", isSettings);
  els.pageSettings.classList.toggle("hidden", !isSettings);
  // 显示后立刻聚焦输入框，让中文输入法候选框贴着光标出现（修复其跑到左上角的问题）
  requestAnimationFrame(() => {
    (isSettings ? els.cfgUrl : els.input).focus();
  });
}

async function doTranslate() {
  const text = els.input.value;
  if (!text.trim()) {
    els.output.value = "";
    return;
  }
  els.status.textContent = "翻译中…";
  els.translateBtn.disabled = true;
  try {
    const result = await invoke("translate", {
      text,
      langA: els.langA.value,
      langB: els.langB.value,
    });
    els.output.value = result;
    els.status.textContent = "";
  } catch (e) {
    els.output.value = "";
    els.status.textContent = String(e);
  } finally {
    els.translateBtn.disabled = false;
  }
}

async function loadConfigIntoUI() {
  const cfg = await invoke("load_config");
  els.cfgUrl.value = cfg.base_url || "";
  els.cfgKey.value = cfg.api_key || "";
  els.cfgModel.value = cfg.model || "";
  els.langA.value = cfg.lang_a || "Chinese";
  els.langB.value = cfg.lang_b || "English";
}

async function saveConfig() {
  const config = {
    base_url: els.cfgUrl.value.trim(),
    api_key: els.cfgKey.value.trim(),
    model: els.cfgModel.value.trim(),
    lang_a: els.langA.value,
    lang_b: els.langB.value,
  };
  try {
    await invoke("save_config", { config });
    els.cfgStatus.textContent = "已保存 ✓";
    setTimeout(() => (els.cfgStatus.textContent = ""), 1500);
  } catch (e) {
    els.cfgStatus.textContent = String(e);
  }
}

// 语言切换后，把当前选择持久化，方便下次启动恢复
async function persistLangs() {
  const cfg = await invoke("load_config");
  cfg.lang_a = els.langA.value;
  cfg.lang_b = els.langB.value;
  await invoke("save_config", { config: cfg });
}

window.addEventListener("DOMContentLoaded", async () => {
  els = {
    card: document.querySelector(".card"),
    pageTranslate: document.querySelector("#page-translate"),
    pageSettings: document.querySelector("#page-settings"),
    langA: document.querySelector("#lang-a"),
    langB: document.querySelector("#lang-b"),
    swap: document.querySelector("#swap"),
    input: document.querySelector("#input"),
    output: document.querySelector("#output"),
    translateBtn: document.querySelector("#translate-btn"),
    status: document.querySelector("#status"),
    cfgUrl: document.querySelector("#cfg-url"),
    cfgKey: document.querySelector("#cfg-key"),
    cfgModel: document.querySelector("#cfg-model"),
    openSettings: document.querySelector("#open-settings"),
    cfgSave: document.querySelector("#cfg-save"),
    cfgBack: document.querySelector("#cfg-back"),
    cfgStatus: document.querySelector("#cfg-status"),
  };

  fillLangSelect(els.langA);
  fillLangSelect(els.langB);
  await loadConfigIntoUI();

  els.translateBtn.addEventListener("click", doTranslate);
  // Ctrl+Enter 快速翻译
  els.input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      doTranslate();
    }
  });

  els.swap.addEventListener("click", async () => {
    const a = els.langA.value;
    els.langA.value = els.langB.value;
    els.langB.value = a;
    // 同时交换输入/输出，方便反向确认
    const t = els.input.value;
    els.input.value = els.output.value;
    els.output.value = t;
    await persistLangs();
  });

  els.langA.addEventListener("change", persistLangs);
  els.langB.addEventListener("change", persistLangs);

  els.openSettings.addEventListener("click", () => showPage("settings"));
  els.cfgSave.addEventListener("click", saveConfig);
  els.cfgBack.addEventListener("click", () => showPage("translate"));

  // Esc 收起浮窗（带下滑动画）
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") slideOutThenHide();
  });

  // 托盘唤起：切到对应页并播放上滑动画
  listen("navigate", (e) => {
    showPage(e.payload);
    slideIn();
  });
  // 后端请求收起（失焦 / 再次点击托盘 / 关闭）：播放下滑动画，结束后隐藏窗口
  listen("flyout-hide", () => slideOutThenHide());

  // 初始停在翻译页（此时窗口隐藏、卡片未滑入）
  showPage("translate");
});
