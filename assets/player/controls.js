/* Remote control input forwarding, keyboard shortcuts, auto-hide bar, and TV/lang toggles. */
(function (global) {
  "use strict";

  function initControls(opts) {
    var doc = global.document;
    var video = opts.video;
    var cursorEl = opts.cursorEl;
    var topBar = opts.topBar;
    var bottomBar = opts.bottomBar;
    var moreMenu = opts.moreMenu;
    var btnPlay = opts.btnPlay;
    var btnMute = opts.btnMute;
    var btnFullscreen = opts.btnFullscreen;
    var btnControl = opts.btnControl;
    var btnMore = opts.btnMore;
    var btnTvMode = opts.btnTvMode;
    var btnLang = opts.btnLang;
    var sendControl = opts.sendControl;
    var showToast = opts.showToast;
    var setState = opts.setState;
    var i18n = opts.i18n;

    var paused = false;
    var controlMode = false;
    var tvMode = false;
    var hideTimer = null;
    var cursorPos = { x: 0.5, y: 0.5 };

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

    function updateCursorDom() {
      cursorEl.style.left = (cursorPos.x * 100) + "%";
      cursorEl.style.top = (cursorPos.y * 100) + "%";
    }

    function setControlMode(enabled) {
      controlMode = enabled;
      btnControl.classList.toggle("active", enabled);
      cursorEl.classList.toggle("visible", enabled);
      updateCursorDom();
      showToast(i18n.t(enabled ? "controlEnabledToast" : "controlDisabledToast"), 2000);
    }

    function mapVideoCoords(clientX, clientY) {
      var r = video.getBoundingClientRect();
      var x = Math.min(1, Math.max(0, (clientX - r.left) / Math.max(1, r.width)));
      var y = Math.min(1, Math.max(0, (clientY - r.top) / Math.max(1, r.height)));
      cursorPos = { x: x, y: y };
      updateCursorDom();
      return cursorPos;
    }

    function togglePause() {
      paused = !paused;
      if (paused) {
        video.pause();
        btnPlay.textContent = "▶";
        setState("paused", i18n.t("pausedTitle"), i18n.t("pausedDesc"), i18n.t("resume"), togglePause);
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
      if (!doc.fullscreenElement) {
        doc.documentElement.requestFullscreen().catch(function () {});
      } else {
        doc.exitFullscreen().catch(function () {});
      }
    }

    video.addEventListener("mousemove", function (e) {
      bumpUiVisibility();
      if (!controlMode) return;
      var p = mapVideoCoords(e.clientX, e.clientY);
      sendControl({ t: "move", x: p.x, y: p.y });
    });

    video.addEventListener("mousedown", function (e) {
      bumpUiVisibility();
      if (!controlMode) return;
      var p = mapVideoCoords(e.clientX, e.clientY);
      cursorEl.classList.add("pressed");
      if (e.button === 2) cursorEl.classList.add("right-click");
      var button = e.button === 2 ? "right" : e.button === 1 ? "middle" : "left";
      sendControl({ t: "click", button: button, x: p.x, y: p.y, double: false });
    });

    video.addEventListener("mouseup", function () {
      cursorEl.classList.remove("pressed", "right-click");
    });

    video.addEventListener("dblclick", function (e) {
      if (controlMode) {
        var p = mapVideoCoords(e.clientX, e.clientY);
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

    doc.addEventListener("keydown", function (e) {
      bumpUiVisibility();
      if (controlMode && e.key.toLowerCase() !== "c") {
        if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].indexOf(e.key) >= 0) {
          e.preventDefault();
          var stepX = 12 / Math.max(640, global.innerWidth);
          var stepY = 12 / Math.max(360, global.innerHeight);
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
        sendControl({
          t: "key",
          code: e.code,
          key: e.key,
          mods: (e.ctrlKey ? 1 : 0) | (e.shiftKey ? 2 : 0) | (e.altKey ? 4 : 0)
        });
        return;
      }

      switch (e.key.toLowerCase()) {
        case "f": toggleFullscreen(); break;
        case "m": toggleMute(); break;
        case "c": setControlMode(!controlMode); break;
        case " ":
          e.preventDefault();
          togglePause();
          break;
        case "escape":
          if (doc.fullscreenElement) doc.exitFullscreen();
          break;
      }
    });

    btnPlay.onclick = togglePause;
    btnMute.onclick = toggleMute;
    btnFullscreen.onclick = toggleFullscreen;
    btnControl.onclick = function () { setControlMode(!controlMode); };
    btnMore.onclick = function () { moreMenu.classList.toggle("open"); };
    btnTvMode.onclick = function () {
      tvMode = !tvMode;
      doc.body.classList.toggle("tv-mode", tvMode);
      btnTvMode.textContent = i18n.t(tvMode ? "on" : "off");
    };
    if (btnLang) {
      btnLang.onclick = function () {
        var next = i18n.getLanguage() === "en" ? "tr" : "en";
        i18n.setLanguage(next, true);
      };
    }

    i18n.onChange(function () {
      btnTvMode.textContent = i18n.t(tvMode ? "on" : "off");
      if (paused) {
        setState("paused", i18n.t("pausedTitle"), i18n.t("pausedDesc"), i18n.t("resume"), togglePause);
      }
    });

    return {
      isPaused: function () { return paused; },
      bumpUiVisibility: bumpUiVisibility
    };
  }

  global.ScreencastControls = { init: initControls };
})(window);
