/* ekran web oynatıcı: MSE + live-edge + gecikme ölçümü + kumanda modu (DESIGN.md & PROTOCOLS.md) */
(function () {
  "use strict";

  const video = document.getElementById("player-video");
  const cursorEl = document.getElementById("cursor-overlay");
  const topBar = document.getElementById("top-bar");
  const bottomBar = document.getElementById("bottom-bar");
  const moreMenu = document.getElementById("more-menu");
  const stateOverlay = document.getElementById("state-overlay");
  const stateSpinner = document.getElementById("state-spinner");
  const stateTitle = document.getElementById("state-title");
  const stateDesc = document.getElementById("state-desc");
  const stateAction = document.getElementById("state-action");
  const toastEl = document.getElementById("toast");

  const deviceNameEl = document.getElementById("device-name");
  const liveDotEl = document.getElementById("live-dot");
  const liveTextEl = document.getElementById("live-text");
  const qualityBadgeEl = document.getElementById("quality-badge");
  const qualityInfoEl = document.getElementById("quality-info");
  const latencyBadgeEl = document.getElementById("latency-badge");

  const btnPlay = document.getElementById("btn-play");
  const btnMute = document.getElementById("btn-mute");
  const btnFullscreen = document.getElementById("btn-fullscreen");
  const btnControl = document.getElementById("btn-control");
  const btnMore = document.getElementById("btn-more");
  const btnTvMode = document.getElementById("tv-mode-toggle");

  const params = new URLSearchParams(window.location.search);
  let token = params.get("token") || "";

  let ws = null;
  let mediaSource = null;
  let sourceBuffer = null;
  let queue = [];
  let isAppending = false;
  let paused = false;
  let controlMode = false;
  let tvMode = false;
  let hideTimer = null;
  let bufferTimer = null;
  let toastTimer = null;
  let lastFragmentAt = 0;
  let bytesInWindow = 0;
  let framesInWindow = 0;
  let cursorPos = { x: 0.5, y: 0.5 };
  let currentQuality = "auto";
  let lastLatencyMs = 0;

  function showToast(msg, durationMs) {
    toastEl.textContent = msg;
    toastEl.classList.add("visible");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
      toastEl.classList.remove("visible");
    }, durationMs || 3000);
  }

  function setState(mode, title, desc, actionLabel, actionFn) {
    if (mode === "live") {
      stateOverlay.classList.remove("visible", "solid");
      liveDotEl.style.background = "var(--live)";
      liveTextEl.textContent = "CANLI";
      return;
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

  function bumpUiVisibility() {
    topBar.classList.remove("hidden");
    bottomBar.classList.remove("hidden");
    clearTimeout(hideTimer);
    hideTimer = setTimeout(function () {
      if (!moreMenu.classList.contains("open") && !paused) {
        topBar.classList.add("hidden");
        bottomBar.classList.add("hidden");
      }
    }, 2500);
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

  function catchLiveEdge() {
    if (!video || paused || !video.buffered || video.buffered.length === 0) return;
    const end = video.buffered.end(video.buffered.length - 1);
    const lag = end - video.currentTime;
    if (lag > 0.35) {
      video.currentTime = Math.max(0, end - 0.10);
    }
    updateLatencyBadge(Math.max(30, lag * 1000 + 60));
  }

  function flushQueue() {
    if (!sourceBuffer || isAppending || queue.length === 0 || sourceBuffer.updating) {
      return;
    }
    isAppending = true;
    const chunk = queue.shift();
    try {
      sourceBuffer.appendBuffer(chunk);
    } catch (err) {
      isAppending = false;
      if (err && err.name === "QuotaExceededError") {
        try {
          if (video.buffered.length > 0) {
            const start = video.buffered.start(0);
            const cut = Math.max(start + 1, video.currentTime - 1);
            if (cut > start && !sourceBuffer.updating) {
              sourceBuffer.remove(start, cut);
              queue.unshift(chunk);
              return;
            }
          }
        } catch (_) {}
      }
    }
  }

  function pickSupportedMime() {
    if (!window.MediaSource) return null;
    const candidates = [
      'video/mp4; codecs="avc1.640028,mp4a.40.2"',
      'video/mp4; codecs="avc1.4D4028,mp4a.40.2"',
      'video/mp4; codecs="avc1.42E01E,mp4a.40.2"',
      'video/mp4; codecs="avc1.640028"',
      'video/mp4; codecs="avc1.42E01E"'
    ];
    for (let i = 0; i < candidates.length; i++) {
      if (MediaSource.isTypeSupported(candidates[i])) {
        return candidates[i];
      }
    }
    return null;
  }

  function setupMse(onReady) {
    const mime = pickSupportedMime();
    if (!mime) {
      if (window.EkranHlsFallback && window.EkranHlsFallback.isSupported()) {
        const hlsUrl = "/hls/stream.m3u8" + (token ? "?token=" + encodeURIComponent(token) : "");
        new window.EkranHlsFallback().attach(video, hlsUrl);
        setState("live");
        showToast("HLS yedek oynatıcı etkin", 3000);
        return;
      }
      setState("error", "Tarayıcı MSE desteklemiyor", "Bu tarayıcı H.264 fMP4 akışını oynatamıyor.");
      return;
    }

    queue = [];
    isAppending = false;
    mediaSource = new MediaSource();
    video.src = URL.createObjectURL(mediaSource);

    mediaSource.addEventListener("sourceopen", function () {
      if (sourceBuffer) return;
      try {
        sourceBuffer = mediaSource.addSourceBuffer(mime);
        sourceBuffer.mode = "sequence";
        sourceBuffer.addEventListener("updateend", function () {
          isAppending = false;
          catchLiveEdge();
          flushQueue();
        });
        onReady();
      } catch (e) {
        setState("error", "Oynatıcı başlatılamadı", String(e), "Yeniden dene", connect);
      }
    });
  }

  function sendControl(payload) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(payload));
    }
  }

  function connect() {
    setState("connecting", "Bağlanılıyor…", "Masaüstü akışı hazırlanıyor.");
    sourceBuffer = null;

    setupMse(function () {
      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      const wsUrl = proto + "//" + window.location.host + "/ws?token=" + encodeURIComponent(token);
      ws = new WebSocket(wsUrl);
      ws.binaryType = "arraybuffer";

      ws.onopen = function () {
        setState("live");
        video.play().catch(function () {});
        bumpUiVisibility();
      };

      ws.onmessage = function (ev) {
        if (ev.data instanceof ArrayBuffer) {
          bytesInWindow += ev.data.byteLength;
          framesInWindow += 1;
          lastFragmentAt = performance.now();
          clearTimeout(bufferTimer);

          if (!paused) {
            if (queue.length > 4) {
              queue.shift();
            }
            queue.push(new Uint8Array(ev.data));
            flushQueue();
            if (!stateOverlay.classList.contains("solid") && stateOverlay.classList.contains("visible")) {
              setState("live");
            }
          }
          return;
        }
        try {
          const msg = JSON.parse(ev.data);
          if (msg.t === "quality" && msg.quality) {
            if (currentQuality !== msg.quality) {
              showToast("↓ " + msg.quality, 4000);
            }
            currentQuality = msg.quality;
            qualityBadgeEl.textContent = msg.quality + (msg.auto ? " (auto)" : "");
            qualityInfoEl.textContent = qualityBadgeEl.textContent;
          } else if (msg.t === "error") {
            if (msg.code === "PortalDenied") {
              setState("denied", "Ekran paylaşım izni verilmedi", msg.message, "Tekrar dene", connect);
            } else {
              setState("error", "Yayın kesildi", msg.message, "Yeniden bağlan", connect);
            }
          }
        } catch (_) {}
      };

      ws.onclose = function (ev) {
        if (ev.code === 4401) {
          setState("error", "Yetkisiz erişim", "URL'deki token parametresini kontrol edin.");
          return;
        }
        setState("error", "Yayın kesildi", "Sunucu bağlantısı kapandı.", "Yeniden bağlan", connect);
      };
    });
  }

  async function fetchInfo() {
    try {
      const url = "/api/info" + (token ? "?token=" + encodeURIComponent(token) : "");
      const res = await fetch(url);
      if (!res.ok) return;
      const info = await res.json();
      if (info.backend) {
        deviceNameEl.textContent = "ekran · " + info.backend;
      }
      if (info.plan && info.plan.quality) {
        currentQuality = info.plan.quality;
        qualityBadgeEl.textContent = info.plan.quality;
        qualityInfoEl.textContent = info.plan.quality;
      }
    } catch (_) {}
  }

  // Her 1 sn'de ağ/gecikme telemetrisini sunucuya bildir & stalling kontrolü yap
  setInterval(function () {
    const kbps = Math.round((bytesInWindow * 8) / 1000);
    const fps = Math.min(60, framesInWindow * 2);
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
    if (lastFragmentAt > 0 && performance.now() - lastFragmentAt > 1200 && !paused && ws && ws.readyState === WebSocket.OPEN) {
      setState("buffering", "Ağ yavaş", "Akış tamponlanıyor…");
    }
  }, 1000);

  // Kumanda modu ve imleç
  function setControlMode(enabled) {
    controlMode = enabled;
    btnControl.classList.toggle("active", enabled);
    cursorEl.classList.toggle("visible", enabled);
    updateCursorDom();
    showToast(enabled ? "Kumanda modu açık (C ile kapat)" : "Kumanda modu kapalı", 2000);
  }

  function updateCursorDom() {
    cursorEl.style.left = (cursorPos.x * 100) + "%";
    cursorEl.style.top = (cursorPos.y * 100) + "%";
  }

  function mapVideoCoords(clientX, clientY) {
    const r = video.getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (clientX - r.left) / Math.max(1, r.width)));
    const y = Math.min(1, Math.max(0, (clientY - r.top) / Math.max(1, r.height)));
    cursorPos = { x: x, y: y };
    updateCursorDom();
    return cursorPos;
  }

  video.addEventListener("mousemove", function (e) {
    bumpUiVisibility();
    if (!controlMode) return;
    const p = mapVideoCoords(e.clientX, e.clientY);
    sendControl({ t: "move", x: p.x, y: p.y });
  });

  video.addEventListener("mousedown", function (e) {
    bumpUiVisibility();
    if (!controlMode) return;
    const p = mapVideoCoords(e.clientX, e.clientY);
    cursorEl.classList.add("pressed");
    if (e.button === 2) cursorEl.classList.add("right-click");
    const button = e.button === 2 ? "right" : e.button === 1 ? "middle" : "left";
    sendControl({ t: "click", button: button, x: p.x, y: p.y, double: false });
  });

  video.addEventListener("mouseup", function () {
    cursorEl.classList.remove("pressed", "right-click");
  });

  video.addEventListener("dblclick", function (e) {
    if (controlMode) {
      const p = mapVideoCoords(e.clientX, e.clientY);
      sendControl({ t: "click", button: "left", x: p.x, y: p.y, double: true });
    } else {
      toggleFullscreen();
    }
  });

  video.addEventListener("contextmenu", function (e) {
    if (controlMode) e.preventDefault();
  });

  video.addEventListener("wheel", function (e) {
    if (!controlMode) return;
    e.preventDefault();
    sendControl({ t: "scroll", dy: e.deltaY > 0 ? 3 : -3 });
  }, { passive: false });

  document.addEventListener("keydown", function (e) {
    bumpUiVisibility();
    if (controlMode && e.key.toLowerCase() !== "c") {
      if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].indexOf(e.key) >= 0) {
        e.preventDefault();
        const stepX = 12 / Math.max(640, window.innerWidth);
        const stepY = 12 / Math.max(360, window.innerHeight);
        if (e.key === "ArrowLeft") cursorPos.x = Math.max(0, cursorPos.x - stepX);
        if (e.key === "ArrowRight") cursorPos.x = Math.min(1, cursorPos.x + stepX);
        if (e.key === "ArrowUp") cursorPos.y = Math.max(0, cursorPos.y - stepY);
        if (e.key === "ArrowDown") cursorPos.y = Math.min(1, cursorPos.y + stepY);
        updateCursorDom();
        sendControl({ t: "move", x: cursorPos.x, y: cursorPos.y });
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        sendControl({ t: "click", button: "left", x: cursorPos.x, y: cursorPos.y, double: false });
        return;
      }
      sendControl({ t: "key", code: e.code, key: e.key, mods: (e.ctrlKey ? 1 : 0) | (e.shiftKey ? 2 : 0) | (e.altKey ? 4 : 0) });
      return;
    }

    switch (e.key.toLowerCase()) {
      case "f":
        toggleFullscreen();
        break;
      case "m":
        toggleMute();
        break;
      case "c":
        setControlMode(!controlMode);
        break;
      case " ":
        e.preventDefault();
        togglePause();
        break;
      case "escape":
        if (document.fullscreenElement) {
          document.exitFullscreen();
        }
        break;
    }
  });

  function togglePause() {
    paused = !paused;
    if (paused) {
      video.pause();
      btnPlay.textContent = "▶";
      setState("paused", "Duraklatıldı", "Devam etmek için Boşluk tuşuna veya ▶ düğmesine basın.", "Devam et", togglePause);
    } else {
      btnPlay.textContent = "⏸";
      setState("live");
      video.play().catch(function () {});
    }
  }

  function toggleMute() {
    video.muted = !video.muted;
    btnMute.textContent = video.muted ? "🔇" : "🔊";
  }

  function toggleFullscreen() {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen().catch(function () {});
    } else {
      document.exitFullscreen().catch(function () {});
    }
  }

  btnPlay.onclick = togglePause;
  btnMute.onclick = toggleMute;
  btnFullscreen.onclick = toggleFullscreen;
  btnControl.onclick = function () { setControlMode(!controlMode); };
  btnMore.onclick = function () { moreMenu.classList.toggle("open"); };
  btnTvMode.onclick = function () {
    tvMode = !tvMode;
    document.body.classList.toggle("tv-mode", tvMode);
    btnTvMode.textContent = tvMode ? "Açık" : "Kapalı";
  };

  fetchInfo();
  connect();
})();
