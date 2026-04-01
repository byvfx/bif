(function () {
  var STORAGE_KEY = "bif-sidebar-fold-state-v1";

  function normalizeText(text) {
    return (text || "")
      .replace(/\s+/g, " ")
      .replace(/[^\x20-\x7E]/g, "")
      .trim()
      .toLowerCase();
  }

  function loadState() {
    try {
      var raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) {
        return {};
      }

      var parsed = JSON.parse(raw);
      if (parsed && typeof parsed === "object") {
        return parsed;
      }
    } catch (e) {
      // Ignore malformed or inaccessible localStorage.
    }

    return {};
  }

  function saveState(state) {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch (e) {
      // Ignore localStorage write failures.
    }
  }

  function getFoldableItems(sidebar) {
    return Array.prototype.filter.call(
      sidebar.querySelectorAll("li.chapter-item"),
      function (item) {
        return !!item.querySelector(":scope > .chapter-link-wrapper .chapter-fold-toggle");
      }
    );
  }

  function computeItemKey(item, index) {
    var wrapper = item.querySelector(":scope > .chapter-link-wrapper");
    var label = wrapper ? normalizeText(wrapper.textContent) : "chapter";
    return String(index) + "|" + label;
  }

  function applyFoldState() {
    var sidebar = document.querySelector("#mdbook-sidebar ol.chapter");
    if (!sidebar) {
      return false;
    }

    var state = loadState();
    var items = getFoldableItems(sidebar);

    items.forEach(function (item, index) {
      var key = computeItemKey(item, index);
      item.setAttribute("data-bif-fold-key", key);

      if (state[key] === true) {
        item.classList.add("expanded");
      } else {
        item.classList.remove("expanded");
      }
    });

    return true;
  }

  function installTogglePersistence() {
    document.addEventListener("click", function (event) {
      var target = event.target;
      if (!target || typeof target.closest !== "function") {
        return;
      }

      var toggle = target.closest(".chapter-fold-toggle");
      if (!toggle) {
        return;
      }

      var chapterItem = toggle.closest("li.chapter-item");
      if (!chapterItem) {
        return;
      }

      window.requestAnimationFrame(function () {
        var key = chapterItem.getAttribute("data-bif-fold-key");
        if (!key) {
          return;
        }

        var state = loadState();
        state[key] = chapterItem.classList.contains("expanded");
        saveState(state);
      });
    });
  }

  function init() {
    if (applyFoldState()) {
      installTogglePersistence();
      return;
    }

    var attempts = 0;
    var timer = setInterval(function () {
      attempts += 1;
      if (applyFoldState() || attempts > 30) {
        clearInterval(timer);
        installTogglePersistence();
      }
    }, 100);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
