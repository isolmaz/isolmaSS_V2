/* Language switch, compact navigation and gentle reveal. No dependencies. */
(function () {
  'use strict';
  var root = document.documentElement;
  var key = 'isolmass.lang';
  function choose(lang, save) {
    if (lang !== 'tr' && lang !== 'en') lang = 'tr';
    root.lang = lang;
    root.setAttribute('data-lang', lang);
    document.querySelectorAll('[data-set-lang]').forEach(function (button) {
      button.setAttribute('aria-pressed', String(button.dataset.setLang === lang));
    });
    var title = root.getAttribute('data-title-' + lang);
    if (title) document.title = title;
    var description = root.getAttribute('data-desc-' + lang);
    var meta = document.querySelector('meta[name="description"]');
    if (description && meta) meta.content = description;
    if (save) { try { localStorage.setItem(key, lang); } catch (_) {} }
  }
  var stored = null;
  try { stored = localStorage.getItem(key); } catch (_) {}
  var query = new URLSearchParams(location.search).get('lang');
  var preferred = (navigator.language || 'tr').slice(0, 2) === 'tr' ? 'tr' : 'en';
  choose(query || stored || preferred, false);
  document.querySelectorAll('[data-set-lang]').forEach(function (button) {
    button.addEventListener('click', function () { choose(button.dataset.setLang, true); });
  });

  var nav = document.querySelector('.nav');
  function onScroll() { if (nav) nav.setAttribute('data-scrolled', String(scrollY > 4)); }
  addEventListener('scroll', onScroll, { passive: true });
  onScroll();

  var toggle = document.querySelector('[data-nav-toggle]');
  var links = document.querySelector('[data-nav-links]');
  if (toggle && links) {
    var open = function (state) {
      links.classList.toggle('is-open', state);
      toggle.setAttribute('aria-expanded', String(state));
    };
    toggle.addEventListener('click', function () { open(toggle.getAttribute('aria-expanded') !== 'true'); });
    links.addEventListener('click', function (event) { if (event.target.closest('a')) open(false); });
    document.addEventListener('keydown', function (event) {
      if (event.key === 'Escape' && toggle.getAttribute('aria-expanded') === 'true') { open(false); toggle.focus(); }
    });
  }

  var reduced = window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (!reduced && 'IntersectionObserver' in window) {
    var observer = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) { entry.target.classList.add('is-in'); observer.unobserve(entry.target); }
      });
    }, { threshold: 0.08 });
    document.querySelectorAll('.reveal').forEach(function (item) { observer.observe(item); });
    root.classList.add('js');
  }
})();
