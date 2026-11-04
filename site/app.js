/* Spud Launcher landing — behaviour only. Motion lives in CSS. */
(() => {
  "use strict";

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  /* ---------- scroll reveal ---------- */

  const revealables = document.querySelectorAll("[data-reveal]");

  if (reduceMotion || !("IntersectionObserver" in window)) {
    revealables.forEach((el) => el.classList.add("is-visible"));
  } else {
    const revealer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          const target = entry.target;
          const delay = Number(target.dataset.revealDelay || 0);
          window.setTimeout(() => target.classList.add("is-visible"), delay);
          revealer.unobserve(target);
        });
      },
      { rootMargin: "0px 0px -12% 0px", threshold: 0.12 }
    );
    revealables.forEach((el) => revealer.observe(el));
  }

  /* ---------- sticky nav ---------- */

  const nav = document.getElementById("nav");
  const onScroll = () => nav && nav.classList.toggle("is-stuck", window.scrollY > 12);
  onScroll();
  window.addEventListener("scroll", onScroll, { passive: true });

  /* ---------- pointer spotlight ---------- */

  if (!reduceMotion) {
    const root = document.documentElement;
    let queued = false;
    window.addEventListener(
      "pointermove",
      (event) => {
        if (queued) return;
        queued = true;
        window.requestAnimationFrame(() => {
          root.style.setProperty("--mx", `${event.clientX}px`);
          root.style.setProperty("--my", `${event.clientY}px`);
          queued = false;
        });
      },
      { passive: true }
    );
  }

  /* ---------- hero parallax ---------- */

  const parallaxLayers = Array.from(document.querySelectorAll("[data-parallax]"));
  const parallaxRoots = Array.from(document.querySelectorAll("[data-parallax-root]"));

  if (!reduceMotion && parallaxLayers.length && parallaxRoots.length) {
    let frame = 0;
    const update = (x, y) => {
      parallaxLayers.forEach((layer) => {
        const depth = Number(layer.dataset.parallax || 0);
        const dx = x * depth;
        const dy = y * depth * 0.6;
        layer.style.transform = `translate3d(${dx}px, ${dy}px, 0)`;
      });
    };

    parallaxRoots.forEach((root) => {
      root.addEventListener("pointermove", (event) => {
        if (frame) return;
        frame = window.requestAnimationFrame(() => {
          const rect = root.getBoundingClientRect();
          const x = (event.clientX - rect.left) / rect.width - 0.5;
          const y = (event.clientY - rect.top) / rect.height - 0.5;
          update(x, y);
          frame = 0;
        });
      });

      root.addEventListener("pointerleave", () => update(0, 0));
    });
  }

  /* ---------- number count-up ---------- */

  const counters = document.querySelectorAll("[data-count]");

  const runCounter = (el) => {
    const target = Number(el.dataset.count || 0);
    if (!Number.isFinite(target)) return;
    if (reduceMotion) {
      el.textContent = String(target);
      return;
    }
    const duration = 1100;
    const start = performance.now();
    const step = (now) => {
      const progress = Math.min((now - start) / duration, 1);
      // easeOutExpo: fast out of the gate, settles on the number
      const eased = progress === 1 ? 1 : 1 - Math.pow(2, -10 * progress);
      el.textContent = String(Math.round(target * eased));
      if (progress < 1) window.requestAnimationFrame(step);
    };
    window.requestAnimationFrame(step);
  };

  if ("IntersectionObserver" in window) {
    const counterObserver = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          runCounter(entry.target);
          counterObserver.unobserve(entry.target);
        });
      },
      { threshold: 0.4 }
    );
    counters.forEach((el) => counterObserver.observe(el));
  }

  /* ---------- fps bars ---------- */

  const bars = document.querySelectorAll(".fps__bar");
  if ("IntersectionObserver" in window) {
    const barObserver = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          entry.target.classList.add("is-visible");
          barObserver.unobserve(entry.target);
        });
      },
      { threshold: 0.35 }
    );
    bars.forEach((bar) => barObserver.observe(bar));
  } else {
    bars.forEach((bar) => bar.classList.add("is-visible"));
  }

  /* ---------- card cursor glow ---------- */

  if (!reduceMotion) {
    document.querySelectorAll(".card").forEach((card) => {
      card.addEventListener("pointermove", (event) => {
        const rect = card.getBoundingClientRect();
        card.style.setProperty("--cx", `${event.clientX - rect.left}px`);
        card.style.setProperty("--cy", `${event.clientY - rect.top}px`);
      });
    });
  }

  /* ---------- lightbox ---------- */

  const lightbox = document.getElementById("lightbox");
  const lightboxImg = document.getElementById("lightbox-img");
  const lightboxClose = document.getElementById("lightbox-close");
  let lastFocus = null;

  const openLightbox = (figure) => {
    const image = figure.querySelector("img");
    if (!image || !lightbox || !lightboxImg) return;
    lastFocus = document.activeElement;
    lightboxImg.src = image.src;
    lightboxImg.alt = image.alt;
    lightbox.hidden = false;
    window.requestAnimationFrame(() => lightbox.classList.add("is-open"));
    document.body.style.overflow = "hidden";
    lightboxClose?.focus();
  };

  const closeLightbox = () => {
    if (!lightbox) return;
    lightbox.classList.remove("is-open");
    document.body.style.overflow = "";
    window.setTimeout(() => {
      lightbox.hidden = true;
      lightboxImg.src = "";
    }, 220);
    if (lastFocus instanceof HTMLElement) lastFocus.focus();
  };

  document.querySelectorAll("[data-lightbox]").forEach((figure) => {
    figure.addEventListener("click", () => openLightbox(figure));
    figure.setAttribute("tabindex", "0");
    figure.setAttribute("role", "button");
    figure.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        openLightbox(figure);
      }
    });
  });

  lightboxClose?.addEventListener("click", closeLightbox);
  lightbox?.addEventListener("click", (event) => {
    if (event.target === lightbox) closeLightbox();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && lightbox && !lightbox.hidden) closeLightbox();
  });
})();
