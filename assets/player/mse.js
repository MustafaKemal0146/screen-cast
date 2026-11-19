/* MediaSource Extensions fMP4 buffer queue, live-edge chasing, and HLS fallback. */
(function (global) {
  "use strict";

  function pickSupportedMime() {
    if (!global.MediaSource) return null;
    var candidates = [
      'video/mp4; codecs="avc1.640028,mp4a.40.2"',
      'video/mp4; codecs="avc1.4D4028,mp4a.40.2"',
      'video/mp4; codecs="avc1.42E01E,mp4a.40.2"',
      'video/mp4; codecs="avc1.640028"',
      'video/mp4; codecs="avc1.42E01E"'
    ];
    for (var i = 0; i < candidates.length; i++) {
      if (MediaSource.isTypeSupported(candidates[i])) {
        return candidates[i];
      }
    }
    return null;
  }

  function createMseController(opts) {
    var video = opts.video;
    var onLatency = opts.onLatency;
    var isPaused = opts.isPaused;
    var onState = opts.onState;
    var onStateKey = opts.onStateKey;
    var onToast = opts.onToast;
    var t = opts.t;

    var mediaSource = null;
    var sourceBuffer = null;
    var queue = [];
    var isAppending = false;

    function catchLiveEdge() {
      if (!video || isPaused() || !video.buffered || video.buffered.length === 0) return;
      var end = video.buffered.end(video.buffered.length - 1);
      var lag = end - video.currentTime;
      if (lag > 0.35) {
        video.currentTime = Math.max(0, end - 0.10);
      }
      onLatency(Math.max(30, lag * 1000 + 60));
    }

    function flushQueue() {
      if (!sourceBuffer || isAppending || queue.length === 0 || sourceBuffer.updating) {
        return;
      }
      isAppending = true;
      var chunk = queue.shift();
      try {
        sourceBuffer.appendBuffer(chunk);
      } catch (err) {
        isAppending = false;
        if (err && err.name === "QuotaExceededError") {
          try {
            if (video.buffered.length > 0) {
              var start = video.buffered.start(0);
              var cut = Math.max(start + 1, video.currentTime - 1);
              if (cut > start && !sourceBuffer.updating) {
                sourceBuffer.remove(start, cut);
                queue.unshift(chunk);
              }
            }
          } catch (_) {}
        }
      }
    }

    function setup(token, onReady, onRetry) {
      var mime = pickSupportedMime();
      if (!mime) {
        if (global.ScreencastHlsFallback && global.ScreencastHlsFallback.isSupported()) {
          var hlsUrl = "/hls/stream.m3u8" + (token ? "?token=" + encodeURIComponent(token) : "");
          new global.ScreencastHlsFallback().attach(video, hlsUrl);
          onState("live");
          onToast(t("hlsFallbackToast"), 3000);
          return;
        }
        if (onStateKey) {
          onStateKey("error", "mseUnsupportedTitle", "mseUnsupportedDesc");
        } else {
          onState("error", t("mseUnsupportedTitle"), t("mseUnsupportedDesc"));
        }
        return;
      }

      sourceBuffer = null;
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
          if (onStateKey) {
            onStateKey("error", "initFailedTitle", null, "retry", onRetry, String(e));
          } else {
            onState("error", t("initFailedTitle"), String(e), t("retry"), onRetry);
          }
        }
      });
    }

    function pushFragment(arrayBuf) {
      var bytes = new Uint8Array(arrayBuf);
      var isInit = bytes.length >= 8 &&
        bytes[4] === 0x66 && bytes[5] === 0x74 && bytes[6] === 0x79 && bytes[7] === 0x70;
      if (isInit) {
        queue = [];
      } else if (queue.length > 4) {
        var dropIdx = queue.findIndex(function (q) {
          return !(q.length >= 8 && q[4] === 0x66 && q[5] === 0x74 && q[6] === 0x79 && q[7] === 0x70);
        });
        if (dropIdx >= 0) queue.splice(dropIdx, 1);
      }
      queue.push(bytes);
      flushQueue();
    }

    return {
      setup: setup,
      pushFragment: pushFragment,
      reset: function () {
        sourceBuffer = null;
        queue = [];
        isAppending = false;
      }
    };
  }

  global.ScreencastMse = { create: createMseController };
})(window);
