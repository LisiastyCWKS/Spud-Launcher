/* ==========================================================================
   Spud Launcher — landing behaviour
   Vanilla ES2020, no dependencies. Everything degrades gracefully: with the
   script blocked the page is static but complete.
   ========================================================================== */
(() => {
  "use strict";

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
  const $ = (sel, scope = document) => scope.querySelector(sel);
  const $$ = (sel, scope = document) => Array.from(scope.querySelectorAll(sel));

  /* ------------------------------------------------------------------------
     Header — shrink once the page leaves the top
     ---------------------------------------------------------------------- */
  const header = $("#header");
  if (header) {
    let stuck = false;
    const syncHeader = () => {
      const next = window.scrollY > 8;
      if (next === stuck) return;
      stuck = next;
      header.classList.toggle("is-stuck", stuck);
    };
    syncHeader();
    window.addEventListener("scroll", syncHeader, { passive: true });
  }

  /* ------------------------------------------------------------------------
     Mobile menu
     ---------------------------------------------------------------------- */
  const menuBtn = $("#menu-btn");
  const mobileMenu = $("#mobile-menu");

  if (menuBtn && mobileMenu) {
    const iconUse = $("use", menuBtn);
    const label = $(".sr-only", menuBtn);

    const setMenu = (open) => {
      mobileMenu.hidden = !open;
      menuBtn.setAttribute("aria-expanded", String(open));
      if (iconUse) iconUse.setAttribute("href", open ? "#i-x" : "#i-menu");
      if (label) label.textContent = open ? "Zamknij menu" : "Otwórz menu";
    };

    menuBtn.addEventListener("click", () => {
      setMenu(mobileMenu.hidden);
    });

    mobileMenu.addEventListener("click", (event) => {
      if (event.target.closest("a")) setMenu(false);
    });

    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !mobileMenu.hidden) {
        setMenu(false);
        menuBtn.focus();
      }
    });

    // The menu only exists below the desktop breakpoint, where .nav is hidden.
    const desktop = window.matchMedia("(min-width: 1041px)");
    desktop.addEventListener("change", (event) => {
      if (event.matches) setMenu(false);
    });
  }

  /* ------------------------------------------------------------------------
     Mockup tabs — click + roving keyboard focus
     ---------------------------------------------------------------------- */
  const tablist = $(".tabs");
  if (tablist) {
    const tabs = $$(".tab", tablist);
    const panels = tabs.map((tab) => document.getElementById(tab.getAttribute("aria-controls")));

    const activate = (index, focus = false) => {
      tabs.forEach((tab, i) => {
        const selected = i === index;
        tab.classList.toggle("is-active", selected);
        tab.setAttribute("aria-selected", String(selected));
        tab.tabIndex = selected ? 0 : -1;
        const panel = panels[i];
        if (panel) {
          panel.hidden = !selected;
          panel.classList.toggle("is-active", selected);
        }
      });
      if (focus) tabs[index].focus();
    };

    tabs.forEach((tab, i) => {
      tab.addEventListener("click", () => activate(i));
    });

    tablist.addEventListener("keydown", (event) => {
      const current = tabs.indexOf(document.activeElement);
      if (current === -1) return;

      let next = null;
      switch (event.key) {
        case "ArrowRight":
        case "ArrowDown":
          next = (current + 1) % tabs.length;
          break;
        case "ArrowLeft":
        case "ArrowUp":
          next = (current - 1 + tabs.length) % tabs.length;
          break;
        case "Home":
          next = 0;
          break;
        case "End":
          next = tabs.length - 1;
          break;
        default:
          return;
      }

      event.preventDefault();
      activate(next, true);
    });
  }

  /* ------------------------------------------------------------------------
     Scroll reveal
     ---------------------------------------------------------------------- */
  const revealables = $$("[data-reveal]");

  if (revealables.length) {
    if (!("IntersectionObserver" in window) || reduceMotion.matches) {
      revealables.forEach((el) => el.classList.add("is-visible"));
    } else {
      const observer = new IntersectionObserver(
        (entries) => {
          entries.forEach((entry) => {
            if (!entry.isIntersecting) return;
            entry.target.classList.add("is-visible");
            observer.unobserve(entry.target);
          });
        },
        { rootMargin: "0px 0px -8% 0px", threshold: 0.12 },
      );
      revealables.forEach((el) => observer.observe(el));

      reduceMotion.addEventListener("change", (event) => {
        if (!event.matches) return;
        revealables.forEach((el) => el.classList.add("is-visible"));
        observer.disconnect();
      });
    }
  }

  /* ------------------------------------------------------------------------
     Feature cards — pointer-tracked glow (single rAF, no debounce)
     ---------------------------------------------------------------------- */
  const features = $$(".feature");

  if (features.length && window.matchMedia("(hover: hover) and (pointer: fine)").matches) {
    let frame = 0;
    let pending = null;

    const paint = () => {
      frame = 0;
      if (!pending) return;
      const { card, x, y } = pending;
      pending = null;
      const rect = card.getBoundingClientRect();
      card.style.setProperty("--mx", `${x - rect.left}px`);
      card.style.setProperty("--my", `${y - rect.top}px`);
    };

    features.forEach((card) => {
      card.addEventListener(
        "pointermove",
        (event) => {
          pending = { card, x: event.clientX, y: event.clientY };
          if (!frame) frame = requestAnimationFrame(paint);
        },
        { passive: true },
      );
    });
  }

  /* ------------------------------------------------------------------------
     Lightbox
     ---------------------------------------------------------------------- */
  const lightbox = $("#lightbox");
  const lightboxImg = $("#lightbox-img");
  const lightboxClose = $("#lightbox-close");
  const shots = $$("[data-lightbox]");

  if (lightbox && lightboxImg && lightboxClose && shots.length) {
    let lastFigure = null;

    const open = (figure) => {
      const img = $("img", figure);
      if (!img) return;
      lastFigure = figure;
      lightboxImg.src = img.currentSrc || img.src;
      lightboxImg.alt = img.alt || "";
      lightbox.hidden = false;
      document.body.style.overflow = "hidden";
      // Force a frame so the transition has a start value.
      requestAnimationFrame(() => lightbox.classList.add("is-open"));
      lightboxClose.focus();
    };

    const close = () => {
      lightbox.classList.remove("is-open");
      document.body.style.overflow = "";
      const finish = () => {
        lightbox.hidden = true;
        lightboxImg.removeAttribute("src");
        lightboxImg.alt = "";
      };
      if (reduceMotion.matches) finish();
      else window.setTimeout(finish, 220);
      if (lastFigure) lastFigure.focus();
      lastFigure = null;
    };

    shots.forEach((figure) => {
      if (!figure.hasAttribute("tabindex")) figure.tabIndex = 0;
      figure.setAttribute("role", "button");
      figure.setAttribute("aria-label", "Powiększ zrzut ekranu");

      figure.addEventListener("click", () => open(figure));
      figure.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        open(figure);
      });
    });

    lightboxClose.addEventListener("click", close);
    lightbox.addEventListener("click", (event) => {
      if (event.target === lightbox) close();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !lightbox.hidden) close();
    });
  }

  /* ------------------------------------------------------------------------
     Release version + footer year
     ---------------------------------------------------------------------- */
  const year = $("[data-year]");
  if (year) year.textContent = String(new Date().getFullYear());

  const releaseSlots = $$("[data-release]");

  if (releaseSlots.length) {
    const fallback = releaseSlots[0].textContent.trim();

    const timeout = (ms) => {
      if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
        return AbortSignal.timeout(ms);
      }
      const controller = new AbortController();
      setTimeout(() => controller.abort(), ms);
      return controller.signal;
    };

    fetch("https://api.github.com/repos/LisiastyCWKS/Spud-Launcher/releases/latest", {
      headers: { accept: "application/vnd.github+json" },
      signal: timeout(4000),
    })
      .then((response) => (response.ok ? response.json() : null))
      .then((data) => {
        const tag = data && typeof data.tag_name === "string" ? data.tag_name.trim() : "";
        if (!tag) return;
        const display = /^v/i.test(tag) ? tag : `v${tag}`;
        releaseSlots.forEach((slot) => {
          slot.textContent = display;
        });
      })
      .catch(() => {
        // Offline, rate-limited or blocked — the static markup already shows
        // a real version, so there is nothing to repair.
        releaseSlots.forEach((slot) => {
          if (!slot.textContent.trim()) slot.textContent = fallback;
        });
      });
  }
})();
