/* screencast web player coordinator: WebSocket session, status overlays, telemetry. */
(function () {
  "use strict";

  var doc = document;
  var i18n = window.ScreencastI18n;
  var video = doc.getElementById("player-video");
  var stateOverlay = doc.getElementById("state-overlay");
  var stateSpinner = doc.getElementById("state-spinner");
  var stateTitle = doc.getElementById("state-title");
  var stateDesc = doc.getElementById("state-desc");
  var stateAction = doc.getElementById("state-action");
  var toastEl = doc.getElementById("toast");

  var deviceNameEl = doc.getElementById("device-name");
  var liveDotEl = doc.getElementById("live-dot");
  var liveTextEl = doc.getElementById("live-text");
  var qualityBadgeEl = doc.getElementById("quality-badge");
  var qualityInfoEl = doc.getElementById("quality-info");
  var latencyBadgeEl = doc.getElementById("latency-badge");

  var params = new URLSearchParams(window.location.search);
  var token = params.get("token") || "";

  var ws = null;
  var toastTimer = null;
  var lastFragmentAt = 0;
  var bytesInWindow = 0;
  var framesInWindow = 0;
  var prevTotalVideoFrames = 0;
  var currentQuality = "auto";
  var currentMode = "connecting";
  var lastState = null;
  var lastLatencyMs = 0;

  var QUALITY_RANK = { "480p": 1, "540p": 2, "720p": 3, "1080p": 4, "1440p": 5 };

  function showToast(msg, durationMs) {
    toastEl.textContent = msg;
    toastEl.classList.add("visible");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
      toastEl.classList.remove("visible");
    }, durationMs || 3000);
  }

  function applyState(mode, title, desc, actionLabel, actionFn) {
    currentMode = mode;
    if (mode === "live") {
      stateOverlay.classList.remove("visible", "solid");
      liveDotEl.style.background = "var(--live)";
      liveTextEl.textContent = i18n.t("liveBadge");
      return;
    }
    if (mode === "connecting") {
      liveTextEl.textContent = i18n.t("connectingBadge");
    }
    stateOverlay.classList.add("visible");
    stateOverlay.classList.toggle("solid", mode === "connecting" || mode === "error" || mode === "denied");
    stateSpinner.style.display = mode === "connecting" || mode === "buffering" ? "block" : "none";
    stateTitle.textContent = title;
    stateDesc.textContent = desc || "";
    if (actionLabel && actionFn) {
      stateAction.hidden = false;
      stateAction.textContent = actionLabel;
      stateAction.onclick = actionFn;
    } else {
      stateAction.hidden = true;
      stateAction.onclick = null;
    }
  }

  function setState(mode, title, desc, actionLabel, actionFn) {
    lastState = null;
    applyState(mode, title, desc, actionLabel, actionFn);
  }

  function setStateKey(mode, titleKey, descKey, actionKey, actionFn, rawDesc) {
    lastState = {
      mode: mode,
      titleKey: titleKey,
      descKey: descKey,
      rawDesc: rawDesc || null,
      actionKey: actionKey || null,
      actionFn: actionFn || null
    };
    var title = titleKey ? i18n.t(titleKey) : "";
    var desc = rawDesc !== undefined && rawDesc !== null ? rawDesc : (descKey ? i18n.t(descKey) : "");
    var actionLabel = actionKey ? i18n.t(actionKey) : null;
    applyState(mode, title, desc, actionLabel, actionFn);
  }

  function updateLatencyBadge(ms) {
    lastLatencyMs = Math.round(ms);
    latencyBadgeEl.textContent = lastLatencyMs + " ms";
    latencyBadgeEl.classList.remove("badge-warn", "badge-danger", "badge-ok");
    if (lastLatencyMs > 700) {
      latencyBadgeEl.classList.add("badge-danger");
    } else if (lastLatencyMs > 400) {
      latencyBadgeEl.classList.add("badge-warn");
    } else {
      latencyBadgeEl.classList.add("badge-ok");
    }
  }

  function sendControl(payload) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(payload));
    }
  }

  var controls = window.ScreencastControls.init({
    video: video,
    cursorEl: doc.getElementById("cursor-overlay"),
    topBar: doc.getElementById("top-bar"),
    bottomBar: doc.getElementById("bottom-bar"),
    moreMenu: doc.getElementById("more-menu"),
    btnPlay: doc.getElementById("btn-play"),
    btnMute: doc.getElementById("btn-mute"),
    btnFullscreen: doc.getElementById("btn-fullscreen"),
    btnControl: doc.getElementById("btn-control"),
    btnMore: doc.getElementById("btn-more"),
    btnTvMode: doc.getElementById("tv-mode-toggle"),
    btnLang: doc.getElementById("lang-toggle"),
    sendControl: sendControl,
    showToast: showToast,
    setState: setState,
    i18n: i18n
  });

  var mse = window.ScreencastMse.create({
    video: video,
    onLatency: updateLatencyBadge,
    isPaused: controls.isPaused,
    onState: setState,
    onStateKey: setStateKey,
    onToast: showToast,
    t: i18n.t
  });

  function connect() {
    setStateKey("connecting", "connectingTitle", "connectingDesc");
    mse.reset();

    mse.setup(token, function () {
      var proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      var wsUrl = proto + "//" + window.location.host + "/ws?token=" + encodeURIComponent(token);
      ws = new WebSocket(wsUrl);
      ws.binaryType = "arraybuffer";

      ws.onopen = function () {
        setState("live");
        video.play().catch(function () {});
        controls.bumpUiVisibility();
      };

      ws.onmessage = function (ev) {
        if (ev.data instanceof ArrayBuffer) {
          bytesInWindow += ev.data.byteLength;
          framesInWindow += 1;
          lastFragmentAt = performance.now();

          if (!controls.isPaused()) {
            mse.pushFragment(ev.data);
            if (!stateOverlay.classList.contains("solid") && stateOverlay.classList.contains("visible")) {
              setState("live");
            }
          }
          return;
        }
        try {
          var msg = JSON.parse(ev.data);
          if (msg.t === "quality" && msg.quality) {
            if (currentQuality !== msg.quality) {
              var prevR = QUALITY_RANK[currentQuality] || 0;
              var nextR = QUALITY_RANK[msg.quality] || 0;
              var arrow = nextR > prevR ? "↑ " : "↓ ";
              showToast(arrow + msg.quality, 4000);
            }
            currentQuality = msg.quality;
            qualityBadgeEl.textContent = msg.quality + (msg.auto ? " (auto)" : "");
            qualityInfoEl.textContent = qualityBadgeEl.textContent;
          } else if (msg.t === "error") {
            if (msg.code === "PortalDenied") {
              setStateKey("denied", "portalDeniedTitle", null, "retry", connect, msg.message);
            } else {
              setStateKey("error", "streamEndedTitle", null, "reconnect", connect, msg.message);
            }
          }
        } catch (_) {}
      };

      ws.onclose = function (ev) {
        if (ev.code === 4401) {
          setStateKey("error", "unauthorizedTitle", "unauthorizedDesc");
          return;
        }
        setStateKey("error", "streamEndedTitle", "streamClosedDesc", "reconnect", connect);
      };
    }, connect);
  }

  async function fetchInfo() {
    try {
      var url = "/api/info" + (token ? "?token=" + encodeURIComponent(token) : "");
      var res = await fetch(url);
      if (!res.ok) return;
      var info = await res.json();
      if (info.language && !i18n.hasExplicitPreference()) {
        i18n.setLanguage(info.language, false);
      }
      if (info.backend) {
        deviceNameEl.textContent = "screencast · " + info.backend;
      }
      if (info.plan && info.plan.quality) {
        currentQuality = info.plan.quality;
        qualityBadgeEl.textContent = info.plan.quality;
        qualityInfoEl.textContent = info.plan.quality;
      }
    } catch (_) {}
  }

  setInterval(function () {
    var kbps = Math.round((bytesInWindow * 8) / 1000);
    var fps = Math.min(60, framesInWindow * 2);
    if (video && typeof video.getVideoPlaybackQuality === "function") {
      var q = video.getVideoPlaybackQuality();
      if (q && typeof q.totalVideoFrames === "number") {
        var delta = Math.max(0, q.totalVideoFrames - prevTotalVideoFrames);
        prevTotalVideoFrames = q.totalVideoFrames;
        if (delta > 0) fps = Math.min(120, delta);
      }
    }
    bytesInWindow = 0;
    framesInWindow = 0;

    if (ws && ws.readyState === WebSocket.OPEN) {
      sendControl({
        t: "stats",
        latencyMs: lastLatencyMs,
        fps: fps,
        measuredKbps: kbps,
        quality: currentQuality
      });
    }
    if (lastFragmentAt > 0 && performance.now() - lastFragmentAt > 2200 && video.readyState < 3 && !controls.isPaused() && ws && ws.readyState === WebSocket.OPEN) {
      setStateKey("buffering", "bufferingTitle", "bufferingDesc");
    }
  }, 1000);

  i18n.onChange(function () {
    if (currentMode === "live") {
      liveTextEl.textContent = i18n.t("liveBadge");
    } else if (lastState && lastState.mode === currentMode && lastState.titleKey) {
      setStateKey(
        lastState.mode,
        lastState.titleKey,
        lastState.descKey,
        lastState.actionKey,
        lastState.actionFn,
        lastState.rawDesc
      );
    }
  });

  i18n.applyStaticDom();
  fetchInfo();
  connect();
})();
