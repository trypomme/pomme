// The app's accessibility tree in ARIA, for the browser's screen reader
// (VoiceOver, NVDA, Narrator, TalkBack…). The worker sends the screen as
// UIAccessibility presents it to VoiceOver (pomme_hle::accessibility::Node:
// windows, groups and elements in reading order); this mirrors it as an
// invisible layer of elements over the screen, each at its element's frame,
// so a screen reader's cursor sits on what it reads. What the screen reader
// does comes back as accessibility actions, which the app handles as
// VoiceOver's: activating an element (its click, or Enter/Space) calls
// accessibilityActivate or taps its activation point, arrows adjust a
// slider, and moving to an element tells it it has VoiceOver's focus.
// Elements are kept by id across updates, so the screen reader's place
// survives the screen changing around it.
'use strict';

const Accessibility = (() => {
  // Actions, as the worker reads them (crates/pomme-web/src/lib.rs: AX_EVENT + action).
  const ACTIVATE = 0, INCREMENT = 1, DECREMENT = 2, FOCUS = 3;
  // Node roles → ARIA. Static text and plain elements are text, read as is.
  const ROLES = {
    window: null, group: 'group', list: 'list', tabBar: 'toolbar', button: 'button', link: 'link', switch: 'switch',
    adjustable: 'slider', textField: 'textbox', searchField: 'searchbox', heading: 'heading', image: 'img',
    staticText: null, element: null,
  };
  // What a keyboard can reach with Tab, as on the web: what does something.
  const ACTIONABLE = new Set(['button', 'link', 'switch', 'adjustable', 'textField', 'searchField']);
  const TRAIT_NOT_ENABLED = 1 << 8, TRAIT_SELECTED = 1 << 3;

  let root = null;
  let act = null; // (action, id) → the worker
  let screen = null; // () → [width, height] in points
  let tree = [];
  const els = new Map(); // id → element

  function attach(layer, sendAction, screenPoints) {
    act = sendAction;
    screen = screenPoints;
    root = document.createElement('div');
    root.id = 'accessibility';
    root.setAttribute('aria-label', 'iPhone screen');
    root.setAttribute('role', 'region');
    layer.appendChild(root);
    root.addEventListener('click', (e) => {
      const n = nodeOf(e.target);
      if (n) { e.preventDefault(); act(ACTIVATE, n.id); }
    });
    root.addEventListener('keydown', (e) => {
      const n = nodeOf(e.target);
      if (!n) return;
      let a = null;
      if (e.key === 'Enter' || (e.key === ' ' && n.role !== 'textField' && n.role !== 'searchField')) a = ACTIVATE;
      else if (n.role === 'adjustable' && (e.key === 'ArrowUp' || e.key === 'ArrowRight')) a = INCREMENT;
      else if (n.role === 'adjustable' && (e.key === 'ArrowDown' || e.key === 'ArrowLeft')) a = DECREMENT;
      if (a !== null) { e.preventDefault(); act(a, n.id); }
    });
    root.addEventListener('focusin', (e) => {
      const n = nodeOf(e.target);
      if (n) act(FOCUS, n.id);
    });
    new ResizeObserver(() => place()).observe(layer);
  }

  function nodeOf(target) {
    const el = target.closest && target.closest('[data-id]');
    return el ? tree.find((n) => String(n.id) === el.dataset.id) || null : null;
  }

  // The screen changed: bring the layer up to date, keeping elements that are still there.
  function update(nodes) {
    tree = nodes;
    const keep = new Set();
    const children = new Map([[null, []]]);
    for (const n of nodes) {
      let el = els.get(n.id);
      if (!el || el.dataset.role !== n.role) {
        if (el) el.remove();
        el = document.createElement('div');
        el.dataset.id = String(n.id);
        el.dataset.role = n.role;
        els.set(n.id, el);
      }
      describe(el, n);
      keep.add(n.id);
      children.set(n.id, children.get(n.id) || []);
      const p = n.parent != null && els.has(n.parent) ? n.parent : null;
      (children.get(p) || children.set(p, []).get(p)).push(el);
    }
    for (const [id, el] of els) if (!keep.has(id)) { el.remove(); els.delete(id); }
    // Children in reading order, moving only what moved (a move loses the screen reader's place).
    for (const [p, kids] of children) {
      const box = p === null ? root : els.get(p);
      kids.forEach((el, i) => {
        if (box.children[i] !== el) box.insertBefore(el, box.children[i] || null);
      });
      while (box.children.length > kids.length) box.lastElementChild.remove();
    }
    place();
  }

  function set(el, name, value) {
    if (value === null || value === undefined || value === false) el.removeAttribute(name);
    else if (el.getAttribute(name) !== String(value)) el.setAttribute(name, String(value));
  }

  // Role, name, value and state, as VoiceOver would say them.
  function describe(el, n) {
    const role = ROLES[n.role];
    const isElement = !['window', 'group', 'list', 'tabBar'].includes(n.role);
    set(el, 'role', role);
    set(el, 'tabindex', ACTIONABLE.has(n.role) ? 0 : isElement ? -1 : null);
    set(el, 'aria-disabled', isElement && n.traits & TRAIT_NOT_ENABLED ? 'true' : null);
    set(el, 'aria-description', n.hint);
    set(el, 'aria-level', n.role === 'heading' ? 2 : null);
    set(el, 'aria-checked', n.role === 'switch' ? String(n.value === '1') : null);
    set(el, 'aria-pressed', n.role === 'button' && n.traits & TRAIT_SELECTED ? 'true' : null);
    set(el, 'aria-current', n.role !== 'button' && isElement && n.traits & TRAIT_SELECTED ? 'true' : null);
    set(el, 'aria-readonly', n.role === 'textField' || n.role === 'searchField' ? 'true' : null);
    if (n.role === 'adjustable') {
      // VoiceOver reads a slider's value as the app says it ("60%"); a number for the rest.
      const num = parseFloat(n.value);
      set(el, 'aria-valuenow', Number.isFinite(num) ? num : 0);
      set(el, 'aria-valuemin', Number.isFinite(num) && n.value.endsWith('%') ? 0 : null);
      set(el, 'aria-valuemax', Number.isFinite(num) && n.value.endsWith('%') ? 100 : null);
      set(el, 'aria-valuetext', n.value || '');
    }
    let text = null;
    if (role === null && isElement) {
      // Text: read as it is, its label then its value.
      text = [n.label, n.value].filter(Boolean).join(', ');
      set(el, 'aria-label', null);
    } else if (n.role === 'textField' || n.role === 'searchField') {
      text = n.value || '';
      set(el, 'aria-label', n.label);
    } else {
      const extra = n.role === 'button' || n.role === 'link' || n.role === 'image' || n.role === 'heading' ? n.value : null;
      set(el, 'aria-label', [n.label, extra].filter(Boolean).join(', ') || null);
    }
    if (text !== null) {
      // Its own text node, before any child elements.
      let t = el.firstChild && el.firstChild.nodeType === Node.TEXT_NODE ? el.firstChild : null;
      if (!t) { t = document.createTextNode(''); el.insertBefore(t, el.firstChild); }
      if (t.data !== text) t.data = text;
    }
  }

  // Each element at its frame (relative to its parent's, as they nest).
  function place() {
    if (!root || !tree.length) return;
    const [w] = screen();
    const k = root.clientWidth / w;
    const byId = new Map(tree.map((n) => [n.id, n]));
    for (const n of tree) {
      const el = els.get(n.id);
      if (!el) continue;
      const p = n.parent != null ? byId.get(n.parent) : null;
      const [x, y, fw, fh] = n.frame;
      const [px, py] = p ? p.frame : [0, 0];
      const s = el.style;
      s.left = `${(x - px) * k}px`;
      s.top = `${(y - py) * k}px`;
      s.width = `${Math.max(fw, 0) * k}px`;
      s.height = `${Math.max(fh, 0) * k}px`;
    }
  }

  return { attach, update };
})();
