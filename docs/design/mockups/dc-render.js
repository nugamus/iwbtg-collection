// A small local renderer for the design canvas's .dc.html artboards (#628).
//
// The canvas on claude.ai renders artboards with its own runtime. This file is
// ours: it implements the part of the format the artboards in this folder use,
// so an artboard opens in any browser served over HTTP (render.py does that)
// and can be exported to a reference PNG. It is not the canvas runtime and
// does not try to be; the canvas stays the source of truth for how an artboard
// looks.
//
// Supported: <x-dc> with <helmet>, {{dotted.path}} holes in text and
// attributes, <sc-for list as>, <sc-if value>, <dc-import name ...props>,
// the <script type="text/x-dc" data-dc-script> logic class (DCLogic with
// props, state, setState, forceUpdate, componentDidMount), event attributes
// bound to functions (onClick="{{ pick }}"), and props from the page's query
// string (?scenario=slower). When every artboard on the page has rendered,
// <html> gets data-dc-ready="1".
(function () {
  'use strict';

  class DCLogic {
    constructor(props) { this.props = props || {}; this.state = {}; }
    setState(next) {
      const patch = typeof next === 'function' ? next(this.state, this.props) : next;
      this.state = Object.assign({}, this.state, patch);
      if (this.__rerender) this.__rerender();
    }
    forceUpdate() { if (this.__rerender) this.__rerender(); }
  }
  window.DCLogic = DCLogic;

  const files = new Map(); // name -> Promise<{template, Component}>
  const headDone = new Set();
  let pending = 0;

  const HOLE = /\{\{\s*([^{}]+?)\s*\}\}/g;
  const WHOLE = /^\s*\{\{\s*([^{}]+?)\s*\}\}\s*$/;

  function literal(expr) {
    if (expr === 'true') return { ok: true, v: true };
    if (expr === 'false') return { ok: true, v: false };
    if (expr === 'null') return { ok: true, v: null };
    if (expr === 'undefined') return { ok: true, v: undefined };
    if (/^-?\d+(\.\d+)?$/.test(expr)) return { ok: true, v: Number(expr) };
    const q = expr.match(/^(['"])(.*)\1$/);
    if (q) return { ok: true, v: q[2] };
    return { ok: false };
  }

  function lookup(scope, expr) {
    const lit = literal(expr);
    if (lit.ok) return lit.v;
    let v = scope;
    for (const part of expr.split('.')) {
      if (v === null || v === undefined) return undefined;
      v = v[part];
    }
    return v;
  }

  function text(v) { return v === null || v === undefined || v === false ? '' : String(v); }

  function interpolate(scope, s) { return s.replace(HOLE, (_, e) => text(lookup(scope, e))); }

  function propName(attr) { return attr.replace(/-([a-z])/g, (_, c) => c.toUpperCase()); }

  function moveHelmet(helmet, key) {
    if (headDone.has(key)) return;
    headDone.add(key);
    for (const n of Array.from(helmet.childNodes)) {
      if (n.nodeType === 1) document.head.appendChild(n.cloneNode(true));
    }
  }

  function makeComponent(src) {
    if (!src || !/class\s+Component/.test(src)) return class extends DCLogic { renderVals() { return {}; } };
    return new Function('DCLogic', src + '\n;return Component;')(DCLogic);
  }

  function parseFile(html) {
    const doc = new DOMParser().parseFromString(html, 'text/html');
    const xdc = doc.querySelector('x-dc');
    const script = doc.querySelector('script[data-dc-script]');
    return { template: xdc, Component: makeComponent(script && script.textContent) };
  }

  function load(name) {
    if (!files.has(name)) {
      files.set(name, fetch(name + '.dc.html').then((r) => {
        if (!r.ok) throw new Error(name + '.dc.html: HTTP ' + r.status);
        return r.text();
      }).then(parseFile));
    }
    return files.get(name);
  }

  // Render the children of `src` with `scope` into `out`.
  async function renderChildren(src, scope, out) {
    for (const n of Array.from(src.childNodes)) await renderNode(n, scope, out);
  }

  async function renderNode(n, scope, out) {
    if (n.nodeType === 3) { out.appendChild(document.createTextNode(interpolate(scope, n.nodeValue))); return; }
    if (n.nodeType !== 1) return;
    const tag = n.localName;
    if (tag === 'helmet') { moveHelmet(n, n.innerHTML); return; }
    if (tag === 'sc-if') {
      const m = (n.getAttribute('value') || '').match(WHOLE);
      if (m ? lookup(scope, m[1]) : false) await renderChildren(n, scope, out);
      return;
    }
    if (tag === 'sc-for') {
      const m = (n.getAttribute('list') || '').match(WHOLE);
      const list = m ? lookup(scope, m[1]) : null;
      const as = n.getAttribute('as') || 'item';
      if (Array.isArray(list)) {
        for (let i = 0; i < list.length; i++) {
          const inner = Object.create(scope);
          inner[as] = list[i];
          inner.$index = i;
          await renderChildren(n, inner, out);
        }
      }
      return;
    }
    if (tag === 'dc-import') {
      const props = {};
      let name = '';
      for (const a of Array.from(n.attributes)) {
        if (a.name === 'name') { name = a.value; continue; }
        if (a.name.startsWith('hint-')) continue;
        const m = a.value.match(WHOLE);
        props[propName(a.name)] = m ? lookup(scope, m[1]) : interpolate(scope, a.value);
      }
      await mount(name, props, out);
      return;
    }
    const el = n.namespaceURI === 'http://www.w3.org/1999/xhtml'
      ? document.createElement(tag)
      : document.createElementNS(n.namespaceURI, n.tagName);
    for (const a of Array.from(n.attributes)) {
      const m = a.value.match(WHOLE);
      const v = m ? lookup(scope, m[1]) : null;
      if (a.name.startsWith('on') && typeof v === 'function') {
        el.addEventListener(a.name.slice(2).toLowerCase(), v);
        continue;
      }
      const value = m ? text(v) : interpolate(scope, a.value);
      if (a.namespaceURI) el.setAttributeNS(a.namespaceURI, a.name, value);
      else el.setAttribute(a.name, value);
    }
    const kids = n.localName === 'template' ? n.content : n;
    await renderChildren(kids, scope, el);
    out.appendChild(el);
  }

  // Mount the artboard `name` (or the page's own, when file is given) with props.
  async function mount(name, props, out, file) {
    pending++;
    try {
      const f = file || await load(name);
      const inst = new f.Component(props);
      inst.props = props;
      const start = document.createComment('dc:' + (name || 'page'));
      const end = document.createComment('/dc');
      out.appendChild(start);
      out.appendChild(end);
      const draw = async () => {
        while (start.nextSibling && start.nextSibling !== end) start.nextSibling.remove();
        const frag = document.createDocumentFragment();
        const vals = (inst.renderVals && inst.renderVals()) || {};
        await renderChildren(f.template, vals, frag);
        end.parentNode.insertBefore(frag, end);
      };
      inst.__rerender = () => { draw(); };
      await draw();
      if (inst.componentDidMount) inst.componentDidMount();
    } catch (e) {
      console.error('dc-render:', name, e);
      const err = document.createElement('pre');
      err.style.cssText = 'color:#ff6070;background:#1a0c10;padding:8px;font:12px monospace;white-space:pre-wrap';
      err.textContent = 'dc-render: ' + (name || 'page') + ': ' + (e && e.message);
      out.appendChild(err);
    } finally {
      pending--;
    }
  }

  async function start() {
    const xdc = document.querySelector('x-dc');
    if (!xdc) return;
    const script = document.querySelector('script[data-dc-script]');
    const file = { template: xdc, Component: makeComponent(script && script.textContent) };
    xdc.remove();
    const props = {};
    for (const [k, v] of new URLSearchParams(location.search)) {
      props[propName(k)] = v === 'true' ? true : v === 'false' ? false : (/^-?\d+(\.\d+)?$/.test(v) ? Number(v) : v);
    }
    const root = document.createElement('div');
    root.className = 'dc-root';
    document.body.appendChild(root);
    await mount('', props, root, file);
    if (document.fonts && document.fonts.ready) await document.fonts.ready;
    // Let images decode before a headless capture.
    await Promise.all(Array.from(document.images).map((img) => img.decode ? img.decode().catch(() => {}) : null));
    document.documentElement.setAttribute('data-dc-ready', pending === 0 ? '1' : 'partial');
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
