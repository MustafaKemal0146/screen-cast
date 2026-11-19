/* English-first runtime i18n helper for the screencast web player. */
(function (global) {
  "use strict";

  var EN_CATALOG = {
    pageTitle: "screencast — Live Stream",
    stageAria: "screencast live stream player",
    controlsAria: "Player controls",
    menuAria: "Stream options",
    liveBadge: "LIVE",
    connectingBadge: "CONNECTING",
    connectingTitle: "Connecting...",
    connectingDesc: "Preparing desktop stream.",
    reconnect: "Reconnect",
    retry: "Retry",
    resume: "Resume",
    playTitle: "Play / Pause (Space)",
    muteTitle: "Mute / Unmute (M)",
    fullscreenTitle: "Fullscreen (F)",
    controlTitle: "Remote control (C)",
    controlLabel: "🖱 Remote",
    moreTitle: "Menu",
    tvModeLabel: "TV Mode (large UI)",
    on: "On",
    off: "Off",
    languageLabel: "Language",
    activeQualityLabel: "Active Quality",
    shortcutsLabel: "Shortcuts",
    shortcutsHint: "F · M · C · Space",
    hlsFallbackToast: "HLS fallback player active",
    mseUnsupportedTitle: "Browser MSE not supported",
    mseUnsupportedDesc: "This browser cannot play H.264 fMP4 streams.",
    initFailedTitle: "Failed to start player",
    portalDeniedTitle: "Screen share permission denied",
    streamEndedTitle: "Stream disconnected",
    streamClosedDesc: "Server connection closed.",
    unauthorizedTitle: "Unauthorized access",
    unauthorizedDesc: "Check the token parameter in the URL.",
    bufferingTitle: "Slow network",
    bufferingDesc: "Buffering stream...",
    pausedTitle: "Paused",
    pausedDesc: "Press Space or ▶ to resume.",
    controlEnabledToast: "Remote control active (press C to exit)",
    controlDisabledToast: "Remote control disabled"
  };

  function normalizeLang(raw) {
    if (!raw || typeof raw !== "string") return null;
    var low = raw.trim().toLowerCase();
    if (low === "tr" || low.indexOf("tr_") === 0 || low.indexOf("tr-") === 0) return "tr";
    if (low === "en" || low.indexOf("en_") === 0 || low.indexOf("en-") === 0) return "en";
    return null;
  }

  function detectInitialLang() {
    var params = new URLSearchParams(global.location.search);
    var urlLang = normalizeLang(params.get("lang"));
    if (urlLang) return urlLang;
    try {
      var saved = normalizeLang(global.localStorage && global.localStorage.getItem("screencast_lang"));
      if (saved) return saved;
    } catch (_) {}
    return "en";
  }

  var currentLang = detectInitialLang();
  var onChangeCbs = [];

  function catalogFor(lang) {
    if (lang === "tr" && global.SCREENCAST_I18N_TR) {
      return global.SCREENCAST_I18N_TR;
    }
    return EN_CATALOG;
  }

  function t(key) {
    var cat = catalogFor(currentLang);
    if (Object.prototype.hasOwnProperty.call(cat, key)) {
      return cat[key];
    }
    return EN_CATALOG[key] || key;
  }

  function applyStaticDom() {
    var doc = global.document;
    if (!doc) return;
    doc.documentElement.lang = currentLang;
    doc.title = t("pageTitle");

    var stage = doc.getElementById("stage");
    if (stage) stage.setAttribute("aria-label", t("stageAria"));
    var bottomBar = doc.getElementById("bottom-bar");
    if (bottomBar) bottomBar.setAttribute("aria-label", t("controlsAria"));
    var moreMenu = doc.getElementById("more-menu");
    if (moreMenu) moreMenu.setAttribute("aria-label", t("menuAria"));

    var map = [
      ["btn-play", "title", "playTitle"],
      ["btn-mute", "title", "muteTitle"],
      ["btn-fullscreen", "title", "fullscreenTitle"],
      ["btn-control", "title", "controlTitle"],
      ["btn-more", "title", "moreTitle"]
    ];
    for (var i = 0; i < map.length; i++) {
      var el = doc.getElementById(map[i][0]);
      if (el) el.setAttribute(map[i][1], t(map[i][2]));
    }

    var textMap = [
      ["btn-control", "controlLabel"],
      ["lbl-tv-mode", "tvModeLabel"],
      ["lbl-language", "languageLabel"],
      ["lbl-active-quality", "activeQualityLabel"],
      ["lbl-shortcuts", "shortcutsLabel"],
      ["shortcuts-hint", "shortcutsHint"]
    ];
    for (var j = 0; j < textMap.length; j++) {
      var tel = doc.getElementById(textMap[j][0]);
      if (tel) tel.textContent = t(textMap[j][1]);
    }
    var langBtn = doc.getElementById("lang-toggle");
    if (langBtn) langBtn.textContent = currentLang.toUpperCase();
  }

  function setLanguage(lang, persist) {
    var norm = normalizeLang(lang) || "en";
    currentLang = norm;
    if (persist) {
      try {
        if (global.localStorage) global.localStorage.setItem("screencast_lang", norm);
      } catch (_) {}
    }
    applyStaticDom();
    for (var i = 0; i < onChangeCbs.length; i++) {
      try { onChangeCbs[i](norm); } catch (_) {}
    }
  }

  global.ScreencastI18n = {
    t: t,
    getLanguage: function () { return currentLang; },
    setLanguage: setLanguage,
    hasExplicitPreference: function () {
      var params = new URLSearchParams(global.location.search);
      if (normalizeLang(params.get("lang"))) return true;
      try {
        return !!(global.localStorage && normalizeLang(global.localStorage.getItem("screencast_lang")));
      } catch (_) {
        return false;
      }
    },
    applyStaticDom: applyStaticDom,
    onChange: function (cb) { onChangeCbs.push(cb); }
  };
})(window);
