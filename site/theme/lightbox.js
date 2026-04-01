(function () {
  function createOverlay() {
    var overlay = document.createElement("div");
    overlay.id = "bif-lightbox";
    overlay.style.cssText =
      "display:none;position:fixed;top:0;left:0;width:100%;height:100%;" +
      "background:rgba(0,0,0,0.92);z-index:9999;cursor:zoom-out;" +
      "justify-content:center;align-items:center;";

    var img = document.createElement("img");
    img.style.cssText =
      "max-width:95%;max-height:95%;object-fit:contain;" +
      "border-radius:4px;box-shadow:0 8px 32px rgba(0,0,0,0.6);";
    overlay.appendChild(img);

    overlay.addEventListener("click", function () {
      overlay.style.display = "none";
    });

    document.body.appendChild(overlay);
    return overlay;
  }

  function init() {
    var overlay = createOverlay();
    var overlayImg = overlay.querySelector("img");

    document.addEventListener("click", function (e) {
      var target = e.target;
      if (target.tagName !== "IMG") return;

      // Only lightbox content images (not sidebar icons, etc.)
      var content = document.getElementById("content");
      if (!content || !content.contains(target)) return;

      overlayImg.src = target.src;
      overlay.style.display = "flex";
    });

    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && overlay.style.display === "flex") {
        overlay.style.display = "none";
      }
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
