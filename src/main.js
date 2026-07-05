const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// value 写入 system prompt（模型理解的英文名）；code 为 BCP-47，用于本地化显示名
const LANGUAGES = [
  { value: "Chinese", code: "zh" },
  { value: "English", code: "en" },
  { value: "Japanese", code: "ja" },
  { value: "Korean", code: "ko" },
  { value: "French", code: "fr" },
  { value: "German", code: "de" },
  { value: "Spanish", code: "es" },
  { value: "Russian", code: "ru" },
  { value: "Italian", code: "it" },
  { value: "Portuguese", code: "pt" },
  { value: "Arabic", code: "ar" },
  { value: "Thai", code: "th" },
  { value: "Vietnamese", code: "vi" },
];

// 界面语言文案。每项的 _name 是该语言的“母语名”，用于下拉显示。
const I18N = {
  zh: {
    _name: "中文",
    swapTitle: "交换语言",
    inputPh: "输入要翻译的内容…",
    outputPh: "翻译结果",
    translate: "翻译",
    settings: "设置",
    settingsTitle: "设置",
    urlLabel: "模型 URL（OpenAI 格式，以 /v1 结尾）",
    keyLabel: "API Key",
    modelLabel: "模型名称",
    uiLangLabel: "界面语言",
    save: "保存",
    back: "返回翻译",
    saved: "已保存 ✓",
    translating: "翻译中…",
  },
  en: {
    _name: "English",
    swapTitle: "Swap languages",
    inputPh: "Enter text to translate…",
    outputPh: "Translation",
    translate: "Translate",
    settings: "Settings",
    settingsTitle: "Settings",
    urlLabel: "Model URL (OpenAI format, ends with /v1)",
    keyLabel: "API Key",
    modelLabel: "Model name",
    uiLangLabel: "UI language",
    save: "Save",
    back: "Back",
    saved: "Saved ✓",
    translating: "Translating…",
  },
  ja: {
    _name: "日本語",
    swapTitle: "言語を入れ替え",
    inputPh: "翻訳する内容を入力…",
    outputPh: "翻訳結果",
    translate: "翻訳",
    settings: "設定",
    settingsTitle: "設定",
    urlLabel: "モデル URL（OpenAI 形式、/v1 で終わる）",
    keyLabel: "API キー",
    modelLabel: "モデル名",
    uiLangLabel: "表示言語",
    save: "保存",
    back: "戻る",
    saved: "保存しました ✓",
    translating: "翻訳中…",
  },
  ko: {
    _name: "한국어",
    swapTitle: "언어 교환",
    inputPh: "번역할 내용을 입력…",
    outputPh: "번역 결과",
    translate: "번역",
    settings: "설정",
    settingsTitle: "설정",
    urlLabel: "모델 URL (OpenAI 형식, /v1로 끝남)",
    keyLabel: "API 키",
    modelLabel: "모델 이름",
    uiLangLabel: "인터페이스 언어",
    save: "저장",
    back: "뒤로",
    saved: "저장됨 ✓",
    translating: "번역 중…",
  },
  fr: {
    _name: "Français",
    swapTitle: "Inverser les langues",
    inputPh: "Saisir le texte à traduire…",
    outputPh: "Traduction",
    translate: "Traduire",
    settings: "Paramètres",
    settingsTitle: "Paramètres",
    urlLabel: "URL du modèle (format OpenAI, se termine par /v1)",
    keyLabel: "Clé API",
    modelLabel: "Nom du modèle",
    uiLangLabel: "Langue de l'interface",
    save: "Enregistrer",
    back: "Retour",
    saved: "Enregistré ✓",
    translating: "Traduction…",
  },
  de: {
    _name: "Deutsch",
    swapTitle: "Sprachen tauschen",
    inputPh: "Zu übersetzenden Text eingeben…",
    outputPh: "Übersetzung",
    translate: "Übersetzen",
    settings: "Einstellungen",
    settingsTitle: "Einstellungen",
    urlLabel: "Modell-URL (OpenAI-Format, endet mit /v1)",
    keyLabel: "API-Schlüssel",
    modelLabel: "Modellname",
    uiLangLabel: "Anzeigesprache",
    save: "Speichern",
    back: "Zurück",
    saved: "Gespeichert ✓",
    translating: "Übersetzen…",
  },
  es: {
    _name: "Español",
    swapTitle: "Intercambiar idiomas",
    inputPh: "Escribe el texto a traducir…",
    outputPh: "Traducción",
    translate: "Traducir",
    settings: "Ajustes",
    settingsTitle: "Ajustes",
    urlLabel: "URL del modelo (formato OpenAI, termina en /v1)",
    keyLabel: "Clave API",
    modelLabel: "Nombre del modelo",
    uiLangLabel: "Idioma de la interfaz",
    save: "Guardar",
    back: "Volver",
    saved: "Guardado ✓",
    translating: "Traduciendo…",
  },
  ru: {
    _name: "Русский",
    swapTitle: "Поменять языки",
    inputPh: "Введите текст для перевода…",
    outputPh: "Перевод",
    translate: "Перевести",
    settings: "Настройки",
    settingsTitle: "Настройки",
    urlLabel: "URL модели (формат OpenAI, оканчивается на /v1)",
    keyLabel: "API-ключ",
    modelLabel: "Название модели",
    uiLangLabel: "Язык интерфейса",
    save: "Сохранить",
    back: "Назад",
    saved: "Сохранено ✓",
    translating: "Перевод…",
  },
};

let currentLang = "zh";
let els = {};

function t(key) {
  return (I18N[currentLang] || I18N.zh)[key];
}

// 按语言刷新所有界面文案
function applyLocale(lang) {
  currentLang = I18N[lang] ? lang : "zh";
  document.documentElement.lang = currentLang;
  els.swap.title = t("swapTitle");
  els.input.placeholder = t("inputPh");
  els.output.placeholder = t("outputPh");
  els.translateBtn.textContent = t("translate");
  els.openSettings.textContent = t("settings");
  els.settingsTitle.textContent = t("settingsTitle");
  els.lblUrl.textContent = t("urlLabel");
  els.lblKey.textContent = t("keyLabel");
  els.lblModel.textContent = t("modelLabel");
  els.lblUiLang.textContent = t("uiLangLabel");
  els.cfgSave.textContent = t("save");
  els.cfgBack.textContent = t("back");
  // A(B) 中的 A 随界面语言变化，需重建两个语种下拉（保留已选值）
  fillLangSelect(els.langA);
  fillLangSelect(els.langB);
}

// 某语言在指定区域设置下的显示名（失败则回退到代码本身）
function displayName(inLocale, code) {
  try {
    return new Intl.DisplayNames([inLocale], { type: "language" }).of(code) || code;
  } catch {
    return code;
  }
}

// 「当前界面语言译名 (该语言母语名)」，两者相同时只显示一个
function langLabel(code) {
  const a = displayName(currentLang, code);
  const b = displayName(code, code);
  return a === b ? a : `${a} (${b})`;
}

function fillLangSelect(sel) {
  const prev = sel.value;
  sel.innerHTML = "";
  for (const l of LANGUAGES) {
    const o = document.createElement("option");
    o.value = l.value;
    o.textContent = langLabel(l.code);
    sel.appendChild(o);
  }
  if (prev) sel.value = prev;
}

// UI 语言下拉：每项用该语言的母语名显示
function fillUiLangSelect(sel) {
  sel.innerHTML = "";
  for (const [code, dict] of Object.entries(I18N)) {
    const o = document.createElement("option");
    o.value = code;
    o.textContent = dict._name;
    sel.appendChild(o);
  }
}

// 收起动画时长，需与 styles.css 中 .card 的 transition 时长保持一致
const CLOSE_MS = 280;
let hideTimer = null;

function setFlyoutOrigin(origin) {
  const fromTop = origin === "top";
  els.card.classList.toggle("from-top", fromTop);
  els.card.classList.toggle("from-bottom", !fromTop);
}

// 卡片从下方滑入（打开）
function slideIn(origin) {
  clearTimeout(hideTimer);
  hideTimer = null;
  setFlyoutOrigin(origin);
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
  els.status.textContent = t("translating");
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
  els.cfgUiLang.value = cfg.ui_lang || "zh";
  applyLocale(els.cfgUiLang.value);
}

async function saveConfig() {
  const config = {
    base_url: els.cfgUrl.value.trim(),
    api_key: els.cfgKey.value.trim(),
    model: els.cfgModel.value.trim(),
    lang_a: els.langA.value,
    lang_b: els.langB.value,
    ui_lang: els.cfgUiLang.value,
  };
  try {
    await invoke("save_config", { config });
    els.cfgStatus.textContent = t("saved");
    setTimeout(() => (els.cfgStatus.textContent = ""), 1500);
  } catch (e) {
    els.cfgStatus.textContent = String(e);
  }
}

// 界面语言切换后立即持久化
async function persistUiLang() {
  const cfg = await invoke("load_config");
  cfg.ui_lang = els.cfgUiLang.value;
  await invoke("save_config", { config: cfg });
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
    cfgUiLang: document.querySelector("#cfg-ui-lang"),
    openSettings: document.querySelector("#open-settings"),
    cfgSave: document.querySelector("#cfg-save"),
    cfgBack: document.querySelector("#cfg-back"),
    cfgStatus: document.querySelector("#cfg-status"),
    settingsTitle: document.querySelector("#settings-title"),
    lblUrl: document.querySelector("#lbl-url"),
    lblKey: document.querySelector("#lbl-key"),
    lblModel: document.querySelector("#lbl-model"),
    lblUiLang: document.querySelector("#lbl-uilang"),
  };

  fillLangSelect(els.langA);
  fillLangSelect(els.langB);
  fillUiLangSelect(els.cfgUiLang);
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

  // 切换界面语言：立即刷新文案并持久化
  els.cfgUiLang.addEventListener("change", async () => {
    applyLocale(els.cfgUiLang.value);
    await persistUiLang();
  });

  // Esc 收起浮窗（带下滑动画）
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") slideOutThenHide();
  });

  // 托盘唤起：切到对应页并播放上滑动画
  listen("navigate", (e) => {
    const payload = e.payload;
    const page = typeof payload === "string" ? payload : payload?.page;
    const origin = typeof payload === "string" ? "bottom" : payload?.origin;
    showPage(page || "translate");
    slideIn(origin || "bottom");
  });
  // 后端请求收起（失焦 / 再次点击托盘 / 关闭）：播放下滑动画，结束后隐藏窗口
  listen("flyout-hide", () => slideOutThenHide());

  // 初始停在翻译页（此时窗口隐藏、卡片未滑入）
  showPage("translate");
});
