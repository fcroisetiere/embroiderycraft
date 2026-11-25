// EmbroideryCraft Library web page. Everything goes through the public API (/api/v1), the same
// one other programs use.
'use strict';

const root = document.getElementById('app');
const PAGE = 60;

const state = {
  me: null,
  q: '',
  tags: [],
  customer: null, // customer id, or 0 for designs without a customer
  kind: '', // 'artwork', 'name', 'all', or '' (artwork only, unless searching or in a customer)
  usage: '',
  archived: '', // '', 'all' or 'only'
  sort: 'recent',
  items: [],
  total: 0,
  allTags: [],
  customers: [],
  usages: [],
  needleCount: 12,
  colourway: {}, // design id → colourway id shown in the drawer
  open: null, // design id shown in the drawer
  view: 'designs', // or 'threads', 'machines'
  machines: [], // the shop's machines, for sending designs
};

// ---------- helpers ----------

/** Make an element: h('div', {class: 'x', onclick: f}, 'text', child, [more]) */
function h(tag, attrs, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'class') el.className = v;
    else if (k === 'value') el.value = v;
    else if (k === 'checked') el.checked = !!v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  const add = (c) => {
    if (c === null || c === undefined || c === false) return;
    if (Array.isArray(c)) c.forEach(add);
    else el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  };
  kids.forEach(add);
  return el;
}

class ApiError extends Error {
  constructor(status, message) { super(message); this.status = status; }
}

async function api(method, path, body) {
  const opts = { method, headers: { 'X-EC': '1' } };
  if (body instanceof FormData) opts.body = body;
  else if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }
  const res = await fetch('/api/v1' + path, opts);
  let data = null;
  try { data = await res.json(); } catch { /* not JSON */ }
  if (!res.ok) {
    if (res.status === 401 && path !== '/session') { showSignIn(); }
    throw new ApiError(res.status, (data && data.error) || res.statusText);
  }
  return data;
}

const num = (n) => (n === null || n === undefined ? '–' : Number(n).toLocaleString());
const mm = (v) => (v === null || v === undefined ? '–' : v.toFixed(1));
function bytes(n) {
  if (n < 1024) return n + ' B';
  if (n < 1024 * 1024) return (n / 1024).toFixed(0) + ' KB';
  return (n / 1024 / 1024).toFixed(1) + ' MB';
}
function facts(d) {
  const parts = [];
  if (d.width_mm != null) parts.push(`${mm(d.width_mm)} × ${mm(d.height_mm)} mm`);
  if (d.stitches != null) parts.push(`${num(d.stitches)} st`);
  if (d.colours) parts.push(`${d.colours} colour${d.colours === 1 ? '' : 's'}`);
  return parts.join(' · ') || 'No stitches';
}
const previewUrl = (d, size, cw, focus) => `/api/v1/designs/${d.id}/preview?size=${size}&v=${d.updated}${cw ? '&colourway=' + cw : ''}${focus != null ? '&focus=' + focus : ''}`;
/** The same thread keeps the same needle: 1, 2, 3… in order of first use. */
/** Common fabric colours, for the background of a colourway. */
const FABRICS = [['White', '#ffffff'], ['Black', '#141414'], ['Navy', '#1c2541'], ['Royal blue', '#2546a8'],
  ['Red', '#b3202a'], ['Grey', '#8c8f94'], ['Yellow', '#f2c80f'], ['Orange', '#e8731c'], ['Green', '#2f7d3a'],
  ['Brown', '#6b4423'], ['Purple', '#5b2c83']];
const fabricName = (rgb) => (FABRICS.find(([, v]) => v === rgb) || [rgb])[0];
/** Inline style putting a preview on its fabric colour (none: the page's plain fabric). */
const fabricStyle = (rgb) => (rgb ? `background-color:${rgb};background-image:none` : '');
/** Whether a thread nearly vanishes on a background (a rough "redmean" colour distance). */
function closeTo(a, b) {
  if (!a || !b) return false;
  const c = (x) => [1, 3, 5].map((i) => parseInt(x.slice(i, i + 2), 16));
  const [p, q] = [c(a), c(b)];
  const r = (p[0] + q[0]) / 2;
  const [dr, dg, db] = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
  return Math.sqrt((2 + r / 256) * dr * dr + 4 * dg * dg + (2 + (255 - r) / 256) * db * db) < 70;
}

function suggestNeedles(threads, count) {
  const seen = [];
  return threads.map((t) => {
    const key = (t.chart && t.code) ? t.chart + '|' + t.code : t.rgb;
    let i = seen.indexOf(key);
    if (i < 0) { seen.push(key); i = seen.length - 1; }
    return (i % count) + 1;
  });
}

/** A button that asks for a second click before doing something that can't be undone. */
function armed(label, armedLabel, onConfirm, cls = 'btn danger small') {
  const b = h('button', { class: cls, type: 'button' }, label);
  let timer = null;
  b.addEventListener('click', async () => {
    if (!b.classList.contains('armed')) {
      b.classList.add('armed');
      b.textContent = armedLabel;
      timer = setTimeout(() => { b.classList.remove('armed'); b.textContent = label; }, 4000);
      return;
    }
    clearTimeout(timer);
    b.disabled = true;
    try { await onConfirm(); } finally { b.disabled = false; }
  });
  return b;
}

/** A message at the bottom of the page for a few seconds. */
function toast(text) {
  const t = h('div', { class: 'toast', role: 'status' }, text);
  document.body.append(t);
  setTimeout(() => t.remove(), 6000);
}

function debounce(f, ms) {
  let t = null;
  return (...a) => { clearTimeout(t); t = setTimeout(() => f(...a), ms); };
}

// ---------- address bar (so a search or a design can be linked) ----------

function readHash() {
  const p = new URLSearchParams(location.hash.slice(1));
  state.q = p.get('q') || '';
  state.tags = (p.get('tags') || '').split(',').filter(Boolean);
  state.sort = p.get('sort') || 'recent';
  state.customer = p.has('c') ? Number(p.get('c')) : null;
  state.kind = p.get('kind') || '';
  state.usage = p.get('usage') || '';
  state.archived = p.get('arch') || '';
  state.open = p.get('d') ? Number(p.get('d')) : null;
  state.view = ['threads', 'machines', 'jobs'].includes(p.get('view')) ? p.get('view') : 'designs';
}
function writeHash() {
  const p = new URLSearchParams();
  if (state.q) p.set('q', state.q);
  if (state.tags.length) p.set('tags', state.tags.join(','));
  if (state.sort !== 'recent') p.set('sort', state.sort);
  if (state.customer !== null) p.set('c', state.customer);
  if (state.kind) p.set('kind', state.kind);
  if (state.usage) p.set('usage', state.usage);
  if (state.archived) p.set('arch', state.archived);
  if (state.open) p.set('d', state.open);
  if (state.view !== 'designs') p.set('view', state.view);
  const s = p.toString();
  history.replaceState(null, '', s ? '#' + s : location.pathname);
}

// ---------- sign in ----------

async function boot() {
  let st;
  try { st = await api('GET', '/status'); } catch (e) {
    root.replaceChildren(h('div', { class: 'center' }, h('p', {}, 'The server does not answer: ' + e.message)));
    return;
  }
  if (st.setup_needed) return showSetup();
  if (!st.user) return showSignIn();
  state.me = st.user;
  try { state.needleCount = (await api('GET', '/settings')).needle_count; } catch { /* keep the default */ }
  loadMachines();
  readHash();
  showLibrary();
  if (state.me.admin) {
    api('GET', '/backup').then((b) => {
      if (!b.plan.folder) toast('The library is not backed up: choose a backup folder in Settings › Backups.');
      else if (b.last && !b.last.ok) toast('The last backup failed: see Settings › Backups.');
    }).catch(() => {});
  }
}

function signinForm(title, intro, button, submit) {
  const name = h('input', { type: 'text', autocomplete: 'username', required: true });
  const pass = h('input', { type: 'password', autocomplete: title === 'Sign in' ? 'current-password' : 'new-password', required: true });
  const err = h('div', { class: 'error', role: 'alert' });
  const form = h('form', { class: 'signin', onsubmit: async (e) => {
    e.preventDefault();
    err.textContent = '';
    try { await submit(name.value, pass.value); boot(); } catch (x) { err.textContent = x.message; }
  } },
    h('div', { class: 'brand' }, h('span', { class: 'dot' }), 'EmbroideryCraft Library'),
    h('h2', {}, title),
    intro && h('p', { class: 'hint' }, intro),
    h('label', {}, 'Name'), name,
    h('label', {}, 'Password'), pass,
    err,
    h('button', { class: 'btn primary', type: 'submit' }, button));
  root.replaceChildren(h('div', { class: 'center' }, form));
  name.focus();
}

function showSetup() {
  signinForm('Welcome', 'Create the administrator account for this library. Other people get their own accounts later, under Settings.',
    'Create and sign in', (name, password) => api('POST', '/setup', { name, password }));
}

function showSignIn() {
  state.me = null;
  signinForm('Sign in', null, 'Sign in', (name, password) => api('POST', '/session', { name, password }));
}

// ---------- the library ----------

let grid, bar, tagList, moreBox, custList, usageList, custHead;

function showLibrary() {
  const search = h('input', {
    type: 'search', placeholder: 'Search names, customers, tags, notes…', value: state.q, 'aria-label': 'Search',
    oninput: debounce((e) => { state.q = e.target.value.trim(); writeHash(); load(); }, 250),
  });
  const sort = h('select', { 'aria-label': 'Sort', onchange: (e) => { state.sort = e.target.value; writeHash(); load(); } },
    [['recent', 'Recently changed'], ['name', 'Name'], ['stitches', 'Most stitches'], ['size', 'Largest']]
      .map(([v, l]) => h('option', { value: v, selected: v === state.sort }, l)));
  const tab = (v, label) => h('button', { class: 'tab' + (state.view === v ? ' on' : ''),
    onclick: () => { if (state.view !== v) { state.view = v; state.open = null; writeHash(); showLibrary(); } } }, label);
  const designs = state.view === 'designs';
  const top = h('div', { class: 'top' },
    h('div', { class: 'brand' }, h('span', { class: 'dot' }), 'EmbroideryCraft Library'),
    h('nav', { class: 'tabs' }, tab('designs', 'Designs'), tab('jobs', 'Jobs'), tab('threads', 'Threads'), tab('machines', 'Machines')),
    designs ? h('div', { class: 'search' }, search) : null,
    designs ? sort : null,
    h('div', { class: 'spacer' }),
    h('button', { class: 'btn primary', onclick: () => showUpload([]) }, '+ Add designs'),
    h('span', { class: 'who' }, state.me.name),
    h('button', { class: 'btn small', onclick: showSettings }, 'Settings'),
    h('button', { class: 'btn small', onclick: async () => { await api('DELETE', '/session'); showSignIn(); } }, 'Sign out'));
  if (!designs) {
    const page = h('div', { class: 'layout' });
    root.replaceChildren(top, page);
    if (state.view === 'machines') showMachines(page);
    else if (state.view === 'jobs') showJobs(page);
    else showThreads(page);
    return;
  }
  custList = h('div', { class: 'tag-list' });
  usageList = h('div', { class: 'tag-list' });
  tagList = h('div', { class: 'tag-list' });
  custHead = h('div', {});
  bar = h('div', { class: 'bar' });
  grid = h('div', { class: 'grid' });
  moreBox = h('div', { class: 'more' });
  root.replaceChildren(top, h('div', { class: 'layout' },
    h('aside', { class: 'side' },
      h('h3', {}, 'Customers'), custList,
      h('h3', {}, 'Usage'), usageList,
      h('h3', {}, 'Tags'), tagList),
    h('main', { class: 'main' }, custHead, bar, grid, moreBox)));
  load();
  if (state.open) openDesign(state.open);
}

// ---------- customers ----------

const CUSTOMERS_SHOWN = 60;
let custFilter = '';
let showClosed = false;

const custLabel = (c) => (c.old_number && c.old_number.toLowerCase() !== c.name.toLowerCase() ? `${c.name} (${c.old_number})` : c.name);
const custById = (id) => state.customers.find((c) => c.id === id);
const childrenOf = (id) => state.customers.filter((c) => c.parent_id === id);
/** Designs of a customer and of every customer inside it. */
function custTotal(c, seen = new Set()) {
  if (seen.has(c.id)) return 0;
  seen.add(c.id);
  return c.designs + childrenOf(c.id).reduce((n, k) => n + custTotal(k, seen), 0);
}
/** The chosen customer and the groups above it. */
function custPath(id) {
  const out = [];
  for (let c = custById(id); c && out.length < 20; c = custById(c.parent_id)) out.unshift(c);
  return out;
}

async function loadCustomers() {
  try { state.customers = await api('GET', '/customers'); } catch { return; }
  renderCustomers();
  renderCustomerHead();
}

function chooseCustomer(id) {
  state.customer = id;
  writeHash();
  load();
}

/** The customer list: the chosen customer's group open, the rest filtered by the box. */
function renderCustomers() {
  const item = (label, count, on, click, depth = 0, cls = '') => h('button', {
    class: 'tag-item' + (on ? ' on' : '') + cls, style: depth ? `padding-left:${8 + depth * 14}px` : null, onclick: click },
    h('span', {}, label), count !== null ? h('span', { class: 'n' }, num(count)) : null);
  const f = custFilter.toLowerCase();
  const open = new Set(custPath(state.customer).map((c) => c.id));
  const rows = [];
  const add = (c, depth) => {
    if (c.archived && !showClosed && !open.has(c.id)) return;
    rows.push(item(custLabel(c), custTotal(c), c.id === state.customer, () => chooseCustomer(c.id === state.customer ? null : c.id), depth,
      (c.archived ? ' closed' : '') + (c.children ? ' group' : '')));
    if (open.has(c.id)) childrenOf(c.id).forEach((k) => add(k, depth + 1));
  };
  if (f) {
    state.customers.filter((c) => (showClosed || !c.archived) && (c.name.toLowerCase().includes(f) || c.old_number.toLowerCase().includes(f)))
      .forEach((c) => rows.push(item(custLabel(c), custTotal(c), c.id === state.customer, () => chooseCustomer(c.id), 0, c.archived ? ' closed' : '')));
  } else {
    state.customers.filter((c) => c.parent_id === null).forEach((c) => add(c, 0));
  }
  const shown = rows.slice(0, CUSTOMERS_SHOWN);
  const box = h('input', { type: 'search', placeholder: `Find a customer (${num(state.customers.length)})`, value: custFilter, 'aria-label': 'Find a customer' });
  box.addEventListener('input', debounce(() => { custFilter = box.value.trim(); renderCustomers(); custList.querySelector('input')?.focus(); }, 150));
  const closed = h('input', { type: 'checkbox', checked: showClosed });
  closed.addEventListener('change', () => { showClosed = closed.checked; renderCustomers(); });
  custList.replaceChildren(
    state.customers.length ? box : '',
    item('All designs', null, state.customer === null, () => chooseCustomer(null)),
    state.customers.length ? item('No customer', null, state.customer === 0, () => chooseCustomer(0)) : '',
    ...shown,
    rows.length > shown.length ? h('p', { class: 'hint' }, `${num(rows.length - shown.length)} more: type to find them.`) : '',
    state.customers.some((c) => c.archived) ? h('label', { class: 'check small' }, closed, 'Show closed customers') : '',
    h('div', { class: 'row', style: 'margin-top:6px' },
      h('button', { class: 'btn small', onclick: () => editCustomer(null) }, '+ Customer'),
      state.customers.some((c) => c.old_number) ? h('button', { class: 'btn small', onclick: nameOldNumbers }, 'Old numbers…') : ''));
}

/** Above the designs: the chosen customer, its groups and a way to change it. */
function renderCustomerHead() {
  if (!custHead) return;
  const path = custPath(state.customer);
  const c = path[path.length - 1];
  if (!c) { custHead.replaceChildren(); return; }
  const kids = childrenOf(c.id);
  custHead.replaceChildren(h('div', { class: 'cust-head' },
    h('div', { class: 'crumbs' }, ...path.slice(0, -1).flatMap((p) => [h('button', { class: 'link', onclick: () => chooseCustomer(p.id) }, custLabel(p)), ' › '])),
    h('div', { class: 'row' },
      h('h2', {}, c.name),
      c.old_number ? h('span', { class: 'badge' }, 'old no. ' + c.old_number) : null,
      c.archived ? h('span', { class: 'badge off' }, 'closed') : null,
      h('div', { class: 'spacer', style: 'flex:1' }),
      h('button', { class: 'btn small', onclick: () => editCustomer(c) }, 'Edit customer')),
    c.notes ? h('p', { class: 'hint' }, c.notes) : null,
    kids.length ? h('div', { class: 'kids' }, h('span', { class: 'hint' }, `${kids.length} inside: `),
      kids.slice(0, 80).map((k) => h('button', { class: 'chip link-chip', onclick: () => chooseCustomer(k.id) }, custLabel(k), h('span', { class: 'n' }, ' ' + num(custTotal(k)))))) : null));
}

/** New customer (c null) or change one. */
function editCustomer(c) {
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  shade.addEventListener('click', close);
  const name = h('input', { type: 'text', value: c ? c.name : '', placeholder: 'e.g. Karaté Soleil Sainte-Julie' });
  const old = h('input', { type: 'text', value: c ? c.old_number : '', placeholder: 'e.g. KSO1A2 (optional)' });
  const group = h('select', {}, h('option', { value: '' }, '— none —'),
    state.customers.filter((g) => !c || g.id !== c.id).map((g) => h('option', { value: g.id, selected: c ? c.parent_id === g.id : state.customer === g.id }, custLabel(g))));
  const notes = h('textarea', { rows: 3, placeholder: 'Contact, usual threads, hoops…' }, c ? c.notes : '');
  const closed = h('input', { type: 'checkbox', checked: c ? c.archived : false });
  const err = h('div', { class: 'error' });
  const save = async () => {
    const body = { name: name.value, old_number: old.value, parent_id: group.value ? Number(group.value) : null, notes: notes.value };
    try {
      if (c) await api('PATCH', '/customers/' + c.id, { ...body, archived: closed.checked });
      else { const n = await api('POST', '/customers', body); state.customer = n.id; writeHash(); }
      close(); load();
    } catch (e) { err.textContent = e.message; }
  };
  const dlg = h('div', { class: 'dialog', role: 'dialog', 'aria-label': c ? 'Customer' : 'New customer' },
    h('h2', {}, c ? custLabel(c) : 'New customer'),
    h('label', {}, 'Name'), name,
    h('label', {}, 'Old customer number'), old,
    h('label', {}, 'Inside the group (franchise network, federation…)'), group,
    h('label', {}, 'Notes'), notes,
    c ? h('label', { class: 'check' }, closed, 'Closed (hidden unless chosen)') : null,
    err,
    h('div', { class: 'actions', style: 'justify-content:space-between' },
      c && state.me.admin ? armed('Delete customer', 'Delete? Its designs stay, without a customer', async () => {
        await api('DELETE', '/customers/' + c.id); close(); chooseCustomer(null);
      }) : h('span', {}),
      h('div', { class: 'row' }, h('button', { class: 'btn', onclick: close }, 'Cancel'), h('button', { class: 'btn primary', onclick: save }, 'Save'))));
  document.body.append(shade, dlg);
  name.focus();
}

/** Give names to customers known only by their old number. */
function nameOldNumbers() {
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  shade.addEventListener('click', close);
  const unnamed = state.customers.filter((c) => c.old_number && c.name.toLowerCase() === c.old_number.toLowerCase());
  const text = h('textarea', { rows: 12, placeholder: 'KSO1A2, Karaté Soleil Sainte-Julie' },
    unnamed.map((c) => `${c.old_number}, `).join('\n'));
  const out = h('div', {});
  const dlg = h('div', { class: 'dialog', role: 'dialog', 'aria-label': 'Old customer numbers' },
    h('h2', {}, 'Names for old customer numbers'),
    h('p', { class: 'hint' }, `${unnamed.length} customers are known only by their old number. Write the name after each number (one per line, number then name, separated by a comma, semicolon or tab; a spreadsheet saved as CSV works). The old number stays searchable.`),
    text, out,
    h('div', { class: 'actions' }, h('button', { class: 'btn', onclick: close }, 'Close'),
      h('button', { class: 'btn primary', onclick: async () => {
        try {
          const lines = text.value.split('\n').filter((l) => /[,;\t]\s*\S/.test(l)).join('\n');
          const r = await api('POST', '/customers/old-numbers', { text: lines });
          out.replaceChildren(h('p', {}, h('strong', {}, `${r.renamed} named.`)),
            r.unknown.length ? h('p', { class: 'hint' }, 'No customer with: ' + r.unknown.join(', ')) : null,
            r.errors.length ? h('ul', {}, r.errors.map((e) => h('li', { class: 'skip' }, e))) : null);
          loadCustomers();
        } catch (e) { out.replaceChildren(h('p', { class: 'error' }, e.message)); }
      } }, 'Save names')));
  document.body.append(shade, dlg);
}

// ---------- usages and kinds ----------

async function loadUsages() {
  try { state.usages = await api('GET', '/usages'); } catch { return; }
  renderUsages();
}

function renderUsages() {
  const item = (label, count, on, click) => h('button', { class: 'tag-item' + (on ? ' on' : ''), onclick: click },
    h('span', {}, label), count !== null ? h('span', { class: 'n' }, num(count)) : null);
  const pick = (u) => { state.usage = state.usage === u ? '' : u; writeHash(); load(); };
  usageList.replaceChildren(
    item('Any', null, !state.usage, () => pick('')),
    ...state.usages.filter((u) => u.designs || u.name === state.usage).map((u) => item(u.name, u.designs, u.name === state.usage, () => pick(u.name))));
}

/** Artwork only by default; both when searching or looking at one customer. */
function effectiveKind() {
  if (state.kind === 'all') return '';
  if (state.kind) return state.kind;
  return state.q || state.customer !== null || state.usage ? '' : 'artwork';
}

const TAGS_SHOWN = 40;
let tagFilter = '';

async function loadTags() {
  try { state.allTags = await api('GET', '/tags'); } catch { return; }
  renderTags();
}

/** The tag list: chosen tags, then the most used ones (or those matching the filter box). */
function renderTags() {
  const item = (label, count, on, click) => h('button', { class: 'tag-item' + (on ? ' on' : ''), onclick: click },
    h('span', {}, label), count !== null ? h('span', { class: 'n' }, num(count)) : null);
  const f = tagFilter.toLowerCase();
  const matching = state.allTags.filter((t) => !state.tags.includes(t.tag) && (!f || t.tag.includes(f)));
  if (!f) matching.sort((a, b) => b.count - a.count);
  const chosen = state.allTags.filter((t) => state.tags.includes(t.tag));
  const shown = matching.slice(0, TAGS_SHOWN);
  const box = h('input', { type: 'search', placeholder: `Find a tag (${num(state.allTags.length)})`, value: tagFilter, 'aria-label': 'Find a tag' });
  box.addEventListener('input', debounce(() => { tagFilter = box.value.trim(); renderTags(); tagList.querySelector('input')?.focus(); }, 150));
  tagList.replaceChildren(
    state.allTags.length > TAGS_SHOWN ? box : '',
    item('Any', null, state.tags.length === 0, () => { state.tags = []; writeHash(); load(); }),
    ...chosen.map((t) => item(t.tag, t.count, true, () => toggleTag(t.tag))),
    ...shown.map((t) => item(t.tag, t.count, false, () => toggleTag(t.tag))),
    matching.length > shown.length ? h('p', { class: 'hint' }, `${num(matching.length - shown.length)} more: type to find them.`) : '');
  if (!state.allTags.length) tagList.append(h('p', { class: 'hint' }, 'Tags you give designs show here.'));
}

function toggleTag(tag) {
  state.tags = state.tags.includes(tag) ? state.tags.filter((t) => t !== tag) : [...state.tags, tag];
  writeHash();
  load();
}

async function load(append = false) {
  const p = new URLSearchParams({ q: state.q, tags: state.tags.join(','), sort: state.sort, limit: PAGE, offset: append ? state.items.length : 0 });
  if (state.customer === 0) p.set('no_customer', 'true');
  else if (state.customer !== null) p.set('customer', state.customer);
  const kind = effectiveKind();
  if (kind) p.set('kind', kind);
  if (state.usage) p.set('usage', state.usage);
  if (state.archived) p.set('archived', state.archived);
  let r;
  try { r = await api('GET', '/designs?' + p); } catch (e) { bar.textContent = e.message; return; }
  state.items = append ? state.items.concat(r.items) : r.items;
  state.total = r.total;
  renderGrid();
  if (!append) { loadTags(); loadCustomers(); loadUsages(); }
}

function renderGrid() {
  const kind = effectiveKind();
  const seg = (v, label) => h('button', { class: 'seg' + ((v === 'all' ? '' : v) === kind ? ' on' : ''),
    onclick: () => { state.kind = v; writeHash(); load(); } }, label);
  const arch = h('select', { 'aria-label': 'Archived designs', style: 'width:auto', onchange: (e) => { state.archived = e.target.value; writeHash(); load(); } },
    [['', 'Current'], ['all', 'Current and archived'], ['only', 'Archived only']].map(([v, l]) => h('option', { value: v, selected: v === state.archived }, l)));
  bar.replaceChildren(
    h('div', { class: 'segs', role: 'group', 'aria-label': 'Type' }, seg('artwork', 'Artwork'), seg('name', 'Names'), seg('all', 'Both')),
    h('span', {}, `${num(state.total)} design${state.total === 1 ? '' : 's'}`),
    state.usage ? h('span', { class: 'chip' }, state.usage, h('button', { 'aria-label': 'Any usage', onclick: () => { state.usage = ''; writeHash(); load(); } }, '×')) :'',
    ...state.tags.map((t) => h('span', { class: 'chip' }, t, h('button', { 'aria-label': 'Remove ' + t, onclick: () => toggleTag(t) }, '×'))),
    h('div', { class: 'spacer', style: 'flex:1' }),
    arch);
  if (!state.items.length) {
    const nothing = !state.q && !state.tags.length && state.customer === null && !state.usage;
    grid.replaceChildren(h('div', { class: 'empty', style: 'grid-column: 1 / -1' },
      h('div', { class: 'big' }, nothing && !kind ? 'The library is empty' : 'No design matches'),
      nothing && !kind ? h('div', {}, 'Drop DST, EMB and picture files on this page, or use ', h('button', { class: 'link', onclick: () => showUpload([]) }, 'Add designs'), '.')
        : h('div', {}, kind ? ['Showing ', kind === 'name' ? 'names' : 'artwork', ' only: try ', h('button', { class: 'link', onclick: () => { state.kind = 'all'; writeHash(); load(); } }, 'both'), '.'] : 'Try fewer words or tags.')));
  } else {
    grid.replaceChildren(...state.items.map(card));
  }
  moreBox.replaceChildren(state.items.length < state.total
    ? h('button', { class: 'btn', onclick: () => load(true) }, `Show more (${num(state.total - state.items.length)} left)`) : '');
}

function card(d) {
  const cust = d.customers.length ? d.customers[0].name + (d.customers.length > 1 ? ` +${d.customers.length - 1}` : '') : '';
  const sub = [cust, d.usage].filter(Boolean).join(' · ');
  return h('div', { class: 'card' + (d.archived ? ' archived' : ''), tabindex: 0, role: 'button', 'aria-label': d.name,
    onclick: () => openDesign(d.id), onkeydown: (e) => { if (e.key === 'Enter') openDesign(d.id); } },
    h('div', { class: 'thumb', style: fabricStyle(d.background) }, d.preview
      ? h('img', { src: previewUrl(d, 320), alt: '', loading: 'lazy' })
      : h('div', { class: 'none' }, 'No preview'),
      d.kind === 'name' ? h('span', { class: 'corner' }, 'Name') : null),
    h('div', { class: 'info' },
      h('div', { class: 'name', title: d.name }, d.name),
      sub ? h('div', { class: 'sub', title: sub }, sub) : null,
      h('div', { class: 'facts' }, facts(d)),
      d.threads.length ? h('div', { class: 'swatches' }, d.threads.slice(0, 14).map((t) => h('span', { class: 'sw', style: `background:${t.rgb}`, title: t.name || t.rgb }))) : null));
}

// ---------- one design ----------

let drawer = null;

function closeDesign() {
  if (drawer) drawer.forEach((el) => el.remove());
  drawer = null;
  state.open = null;
  writeHash();
}

async function openDesign(id) {
  let d;
  try { d = await api('GET', '/designs/' + id); } catch (e) {
    if (e.status === 404) { closeDesign(); return; }
    throw e;
  }
  state.open = id;
  writeHash();
  const old = drawer;
  const shade = h('div', { class: 'shade', onclick: closeDesign });
  const panel = h('div', { class: 'drawer', role: 'dialog', 'aria-label': d.name }, designBody(d));
  if (old) old.forEach((el) => el.remove());
  drawer = [shade, panel];
  document.body.append(shade, panel);
  // the preview stays in view under the header while the colours scroll
  const head = panel.querySelector('header');
  if (head) panel.style.setProperty('--drawer-head', head.offsetHeight + 'px');
}

/** Save a change to the design, then show it again (in the drawer and the grid). */
async function change(d, fields, note) {
  const nd = await api('PATCH', '/designs/' + d.id, fields);
  Object.assign(d, nd);
  const i = state.items.findIndex((x) => x.id === d.id);
  if (i >= 0) { state.items[i] = nd; renderGrid(); }
  if (fields.tags) loadTags();
  if ('customer_ids' in fields || fields.usage !== undefined) { loadCustomers(); loadUsages(); }
  if (note) { note.textContent = 'Saved'; setTimeout(() => { note.textContent = ''; }, 1500); }
  return nd;
}

function designBody(d) {
  const nameIn = h('input', { type: 'text', value: d.name, 'aria-label': 'Name' });
  const saveName = async () => {
    const v = nameIn.value.trim();
    if (v && v !== d.name) { d.name = v; await change(d, { name: v }); }
  };
  nameIn.addEventListener('change', saveName);
  nameIn.addEventListener('keydown', (e) => { if (e.key === 'Enter') nameIn.blur(); });

  const header = h('header', {}, nameIn, h('button', { class: 'btn small', onclick: closeDesign, 'aria-label': 'Close' }, '✕'));

  const cws = d.colourways || [];
  let cwId = state.colourway[d.id];
  if (!cws.some((c) => c.id === cwId)) cwId = null;
  const cw = cws.find((c) => c.id === cwId) || null;
  const colours = cw ? cw.threads : d.threads;
  const needles = cw ? cw.needles : d.needles;
  const showCw = (id) => { state.colourway[d.id] = id; openDesign(d.id); };
  /** Save threads or needles of what is shown: the main colours or the colourway. */
  const saveColours = async (fields, note) => {
    if (cw) {
      const nd = await api('PATCH', '/colourways/' + cw.id, fields);
      Object.assign(d, nd);
      if (note) { note.textContent = 'Saved'; setTimeout(() => { note.textContent = ''; }, 1500); }
    } else await change(d, fields, note);
  };

  const background = cw ? cw.background : d.background;
  const bigImg = d.preview ? h('img', { src: previewUrl(d, 1000, cwId), alt: 'Preview of ' + d.name }) : null;
  const focusNote = h('div', { class: 'focus-note', hidden: true });
  const big = h('div', { class: 'big', style: fabricStyle(background) }, bigImg
    || h('div', { class: 'hint' }, 'No preview: add a DST or EmbroideryCraft file (or a picture) to this design.'), focusNote);
  /** Show one colour in the preview (the others faded), or all again with null. */
  let focusTimer = null;
  let focused = null;
  const showFocus = (i) => {
    if (!bigImg || !d.stitches || i === focused) return;
    clearTimeout(focusTimer);
    focusTimer = setTimeout(() => {
      focused = i;
      bigImg.src = previewUrl(d, 1000, cwId, i);
      focusNote.hidden = i == null;
      if (i != null) focusNote.textContent = `Colour ${i + 1}`;
    }, i == null ? 250 : 120);
  };

  // files
  const addInput = h('input', { type: 'file', multiple: true, hidden: true, onchange: async () => {
    const fd = new FormData();
    for (const f of addInput.files) fd.append('file', f, f.name);
    try {
      const r = await api('POST', `/designs/${d.id}/files`, fd);
      const msgs = r.skipped.concat(r.warnings);
      if (msgs.length) toast(msgs.map((s) => `${s.name}: ${s.reason}`).join('\n'));
    } catch (e) { toast(e.message); }
    openDesign(d.id);
    load();
  } });
  const kindLabel = (f) => ({ stitches: 'Machine', project: 'EmbroideryCraft', picture: 'Picture' }[f.kind]
    || (f.name.includes('.') ? f.name.split('.').pop().toUpperCase() : 'File'));
  const files = h('div', { class: 'files' }, d.files.map((f) => h('div', { class: 'file' },
    h('span', { class: 'kind' }, kindLabel(f)),
    h('span', { class: 'fname', title: f.name }, f.name),
    h('span', { class: 'size' }, bytes(f.size)),
    h('a', { class: 'btn small', href: `/api/v1/files/${f.id}`, download: f.name }, 'Download'),
    armed('Remove', 'Remove?', async () => {
      const r = await api('DELETE', '/files/' + f.id);
      if (r.design_kept) openDesign(d.id); else closeDesign();
      load();
    }))));

  // facts
  const row = (k, v) => h('tr', {}, h('td', {}, k), h('td', {}, v));
  const factTable = d.stitches != null ? h('table', { class: 'facts' },
    row('Size', `${mm(d.width_mm)} × ${mm(d.height_mm)} mm`),
    row('Stitches', num(d.stitches)),
    row('Colours', num(d.colours)),
    row('Trims', num(d.trims)),
    row('Jumps', num(d.jumps)),
  ) : h('p', { class: 'hint' }, 'No stitch file: size and counts show once a DST or EmbroideryCraft file is added.');

  // speed and cost
  const costNote = h('span', { class: 'saved' });
  const e = d.estimate;
  const money = (v) => `${v.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${e ? e.currency : ''}`.trim();
  const speedIn = h('input', { type: 'number', min: 50, max: 3000, step: 10, class: 'speed', value: d.speed ?? '', placeholder: e ? e.speed : '',
    'aria-label': 'Speed in stitches a minute' });
  speedIn.addEventListener('change', async () => {
    const v = speedIn.value.trim();
    try { await change(d, { speed: v ? Number(v) : null }, costNote); openDesign(d.id); } catch (x) { toast(x.message); speedIn.value = d.speed ?? ''; }
  });
  const qty = h('input', { type: 'number', min: 1, value: 1, class: 'speed', 'aria-label': 'Quantity' });
  const totals = h('span', {});
  const showTotals = () => {
    const n = Math.max(1, Number(qty.value) || 1);
    totals.textContent = e ? `${money(e.cost * n)} cost · ${money(e.price * n)} price · ${Math.ceil(e.minutes_per_piece * n)} min` : '';
  };
  qty.addEventListener('input', showTotals);
  showTotals();
  const costTable = e ? h('table', { class: 'facts' },
    h('tr', {}, h('td', {}, 'Speed (st/min)'), h('td', {}, speedIn)),
    row('Sewing time (one run)', `${e.sew_minutes} min`),
    row('Time per piece', `${e.minutes_per_piece} min`),
    row('Cost per piece', money(e.cost)),
    row('Suggested price', money(e.price)),
    h('tr', {}, h('td', {}, 'Quantity ', qty), h('td', {}, totals))) : null;

  // colourways, threads and needles
  const threadNote = h('span', { class: 'saved' });
  const cwTabs = h('div', { class: 'segs cw-tabs', role: 'group', 'aria-label': 'Colourway' },
    h('button', { class: 'seg' + (cw ? '' : ' on'), onclick: () => showCw(null) }, 'Main colours'),
    cws.map((c) => h('button', { class: 'seg' + (c.id === cwId ? ' on' : ''), onclick: () => showCw(c.id) },
      c.background ? h('span', { class: 'fabric-dot', style: `background:${c.background}` }) : null, c.name)),
    h('button', { class: 'seg', title: 'Another set of colours for these stitches (on black, on white…)', onclick: () => {
      if (cwNew.hidden) { cwNew.hidden = false; cwName.focus(); } else makeCw('');
    } }, '+ Colourway'));
  if (d.background) cwTabs.firstChild.prepend(h('span', { class: 'fabric-dot', style: `background:${d.background}` }));
  const makeCw = async (bg) => {
    const name = cwName.value.trim() || (bg ? `On ${fabricName(bg).toLowerCase()}` : '');
    if (!name) { cwName.focus(); toast('Name the colourway, or pick its background.'); return; }
    try {
      const nd = await api('POST', `/designs/${d.id}/colourways`, { name, background: bg });
      const made = nd.colourways[nd.colourways.length - 1];
      showCw(made && made.id);
    } catch (e) { toast(e.message); }
  };
  const cwName = h('input', { type: 'text', placeholder: 'Name, e.g. Club red', 'aria-label': 'New colourway name' });
  cwName.addEventListener('keydown', (e) => { if (e.key === 'Enter') makeCw(''); });
  const cwNew = h('div', { class: 'cw-new', hidden: true }, cwName,
    h('div', { class: 'hint' }, 'Pick its background to make it (named after the colour if you leave the name empty):'),
    h('div', { class: 'fabrics' }, FABRICS.map(([label, rgb]) => h('button', { class: 'fabric', style: `background:${rgb}`,
      title: label, 'aria-label': `New colourway on ${label}`, onclick: () => makeCw(rgb) }))),
    h('button', { class: 'btn small', onclick: () => makeCw('') }, 'Make it without a background'));

  // the fabric under what is shown
  const setBackground = async (rgb) => {
    try { await saveColours({ background: rgb }, threadNote); } catch (e) { toast(e.message); return; }
    openDesign(d.id);
    if (!cw) load();
  };
  const customBg = h('input', { type: 'color', value: background || '#ffffff', title: 'Another colour', 'aria-label': 'Another background colour' });
  customBg.addEventListener('change', () => setBackground(customBg.value));
  const backgroundRow = h('div', { class: 'bg-row' },
    h('span', { class: 'bg-label' }, 'Background', h('span', { class: 'hint' }, background ? ` ${fabricName(background)}` : ' not set')),
    h('div', { class: 'fabrics' },
      FABRICS.map(([label, rgb]) => h('button', { class: 'fabric' + (rgb === background ? ' on' : ''), style: `background:${rgb}`,
        title: label, 'aria-label': `Background ${label}`, 'aria-pressed': String(rgb === background), onclick: () => setBackground(rgb) })),
      customBg,
      background ? h('button', { class: 'link', onclick: () => setBackground('') }, 'None') : null));
  const needleIns = [];
  const saveNeedles = async () => {
    const vals = needleIns.map((el) => el.value.trim());
    const list = vals.every((v) => v === '') ? [] : vals.map((v, i) => Number(v) || Number(needleIns[i].placeholder));
    try { await saveColours({ needles: list }, threadNote); } catch (e) { toast(e.message); openDesign(d.id); }
  };
  const auto = suggestNeedles(colours, state.needleCount);
  const threads = h('div', { class: 'threads', onmouseleave: () => showFocus(null), onfocusout: (e) => { if (!threads.contains(e.relatedTarget)) showFocus(null); } }, colours.map((t, i) => {
    const pick = () => pickThread(d, i, cw, d.stitches && d.preview ? previewUrl(d, 1000, cwId, i) : null);
    const needle = h('input', { type: 'number', min: 1, max: state.needleCount, class: 'needle', value: needles[i] ?? '', placeholder: auto[i],
      title: 'Needle on the machine', 'aria-label': `Needle for colour ${i + 1}` });
    needle.addEventListener('change', saveNeedles);
    needleIns.push(needle);
    return h('div', { class: 'thread', onmouseenter: () => showFocus(i), onfocusin: () => showFocus(i) }, h('span', { class: 'num' }, i + 1),
      h('button', { class: 'swatch-btn', style: `background:${t.rgb}`, title: 'Choose the thread', 'aria-label': `Choose thread ${i + 1}`, onclick: pick }),
      h('button', { class: 'thread-label', onclick: pick },
        t.chart ? [h('strong', {}, t.code), h('span', { class: 'hint' }, ` ${t.chart}${t.name ? ' · ' + t.name : ''}`)]
          : h('span', { class: 'hint' }, 'Choose a thread…')),
      closeTo(t.rgb, background) ? h('span', { class: 'faint', title: 'This thread is close to the background colour' }, 'hard to see') : null,
      h('label', { class: 'needle-label' }, 'Needle', needle));
  }));
  const matchAll = cw ? null : armed('Match all to the default chart', 'Replace every colour with its closest thread?', async () => {
    try { await api('POST', `/designs/${d.id}/match`, {}); } catch (e) { toast(e.message); return; }
    openDesign(d.id);
    load();
  }, 'btn small');
  const cwQ = cwId ? '&colourway=' + cwId : '';
  // administrators: the design as an Odoo product (a variant per colourway), to choose in
  // nomenclatures
  const publishBox = state.me.admin && d.stitches ? h('div', { class: 'publish' }) : null;
  if (publishBox) {
    (async () => {
      let r;
      try { r = await api('GET', `/designs/${d.id}/published`); } catch { return; }
      if (!r.connections.length) return;
      const mine = r.products[0];
      const n = cws.length;
      const btn = h('button', { class: 'btn small', title: 'A product with the preview, cost and suggested price' + (n ? `, and a variant for the main colours and each colourway (${n})` : ''), onclick: async () => {
        btn.disabled = true; btn.textContent = 'Publishing…';
        try {
          const out = await api('POST', `/designs/${d.id}/publish`, {});
          toast(`In Odoo as ${out.product}` + (out.variants.length > 1 ? `, ${out.variants.length} variants` : '') + '. Add it to a nomenclature as a component, “Consumed in” the embroidery operation.');
        } catch (e) { toast(e.message); }
        openDesign(d.id);
      } }, mine ? 'Update in Odoo' : 'Publish to Odoo');
      publishBox.replaceChildren(btn, mine ? h('span', { class: 'hint' }, ` In Odoo as ${mine.label}`) : null);
    })();
  }
  const machine = h('div', { class: 'row', style: 'margin-top:10px' },
    needlesFromMachine(d, cw),
    h('a', { class: 'btn small', href: `/api/v1/designs/${d.id}/machine-file?format=tbf${cwQ}` }, 'Download TBF (with needles)'),
    h('a', { class: 'btn small', href: `/api/v1/designs/${d.id}/machine-file?format=dst${cwQ}` }, 'Download DST'),
    cw ? armed('Delete colourway', `Delete "${cw.name}"?`, async () => {
      const nd = await api('DELETE', '/colourways/' + cw.id); Object.assign(d, nd); showCw(null);
    }, 'btn small danger') : null);

  // customer, type, usage
  const placeNote = h('span', { class: 'saved' });
  // the customers using this design: one logo can serve several
  const setCustomers = async (ids) => {
    try { await change(d, { customer_ids: ids }, placeNote); } catch (e) { toast(e.message); }
    openDesign(d.id);
    loadCustomers();
  };
  const custChips = h('div', { class: 'chips' }, d.customers.map((c) => h('span', { class: 'chip' },
    h('button', { class: 'link', title: 'Show this customer\'s designs', onclick: () => { closeDesign(); chooseCustomer(c.id); } }, c.name),
    h('button', { 'aria-label': 'Remove ' + c.name, title: 'Not this customer\'s design', onclick: () => setCustomers(d.customers.filter((x) => x.id !== c.id).map((x) => x.id)) }, '×'))),
    d.customers.length ? null : h('span', { class: 'hint' }, 'No customer'));
  const custIn = h('input', { type: 'text', list: 'customer-options', placeholder: d.customers.length ? 'Add another customer…' : 'Add a customer…', 'aria-label': 'Add a customer' });
  custIn.addEventListener('change', async () => {
    const v = custIn.value.trim();
    if (!v) return;
    const c = state.customers.find((x) => custLabel(x) === v) || state.customers.find((x) => x.name.toLowerCase() === v.toLowerCase() || x.old_number.toLowerCase() === v.toLowerCase());
    if (!c) { toast(`No customer "${v}": create it under Customers first.`); custIn.value = ''; return; }
    if (d.customers.some((x) => x.id === c.id)) { custIn.value = ''; return; }
    await setCustomers(d.customers.map((x) => x.id).concat(c.id));
  });
  const kindSel = h('select', { 'aria-label': 'Type', onchange: (e) => change(d, { kind: e.target.value }, placeNote) },
    h('option', { value: 'artwork', selected: d.kind !== 'name' }, 'Artwork (logo, crest, lettering)'),
    h('option', { value: 'name', selected: d.kind === 'name' }, 'Name (one person)'));
  const usageNames = state.usages.map((u) => u.name);
  if (d.usage && !usageNames.includes(d.usage)) usageNames.push(d.usage);
  const usageSel = h('select', { 'aria-label': 'Usage', onchange: (e) => change(d, { usage: e.target.value }, placeNote) },
    h('option', { value: '' }, '—'), usageNames.map((u) => h('option', { value: u, selected: u === d.usage }, u)));
  const archIn = h('input', { type: 'checkbox', checked: d.archived });
  archIn.addEventListener('change', () => change(d, { archived: archIn.checked }, placeNote));
  const placeBox = h('div', { class: 'place' },
    h('label', {}, 'Customers'), custChips, custIn,
    h('datalist', { id: 'customer-options' }, state.customers.map((c) => h('option', { value: custLabel(c) }))),
    h('div', { class: 'row', style: 'margin-top:6px' },
      h('div', { style: 'flex:1' }, h('label', {}, 'Type'), kindSel),
      h('div', { style: 'flex:1' }, h('label', {}, 'Usage'), usageSel)),
    h('label', { class: 'check', style: 'margin-top:8px' }, archIn, 'Archived (old work, hidden unless asked for)'));

  // tags
  const tagBox = h('div', { class: 'tags-edit' });
  const tagNote = h('span', { class: 'saved' });
  const listId = 'tag-options';
  const renderTags = () => {
    const input = h('input', { type: 'text', placeholder: 'Add a tag', list: listId, 'aria-label': 'Add a tag' });
    input.addEventListener('keydown', async (e) => {
      if (e.key !== 'Enter' && e.key !== ',') return;
      e.preventDefault();
      const t = input.value.trim().toLowerCase();
      if (!t || d.tags.includes(t)) { input.value = ''; return; }
      d.tags = [...d.tags, t].sort();
      await change(d, { tags: d.tags }, tagNote);
      renderTags();
      tagBox.querySelector('input').focus();
    });
    tagBox.replaceChildren(
      ...d.tags.map((t) => h('span', { class: 'chip' }, t, h('button', { 'aria-label': 'Remove ' + t, onclick: async () => {
        d.tags = d.tags.filter((x) => x !== t);
        await change(d, { tags: d.tags }, tagNote);
        renderTags();
      } }, '×'))),
      input,
      h('datalist', { id: listId }, state.allTags.map((t) => h('option', { value: t.tag }))));
  };
  renderTags();

  // notes
  const notesNote = h('span', { class: 'saved' });
  const notes = h('textarea', { rows: 4, placeholder: 'Customer, fabric, backing, hoop, thread brand…', 'aria-label': 'Notes' }, d.notes);
  notes.addEventListener('change', () => change(d, { notes: notes.value }, notesNote));

  const remove = armed('Delete design', 'Delete it and its files?', async () => {
    await api('DELETE', '/designs/' + d.id);
    closeDesign();
    load();
  });

  const colourBox = d.threads.length ? [h('h3', {}, 'Colours and needles ', threadNote), cwTabs, cwNew, backgroundRow, threads,
    h('p', { class: 'hint' }, `Point at a colour to see it in the design. DST files don't store colours or needles: choose the thread and the needle (1–${state.needleCount}) for each colour change. A grey needle number is a suggestion (the same thread keeps the same needle). A colourway is the same design in other colours, e.g. for a black or a white background: give each one its background, and threads too close to it are marked.`),
    matchAll, machine, publishBox] : null;
  return [header, h('div', { class: 'body' },
    h('div', {},
      h('div', { class: 'pin' }, big),
      colourBox,
      d.stitches ? sendRow(d, cw) : null,
      h('h3', {}, 'Files'),
      files,
      h('div', { class: 'row', style: 'margin-top:8px' }, addInput,
        h('button', { class: 'btn small', onclick: () => addInput.click() }, '+ Add files'),
        h('span', { class: 'hint' }, 'EMB and other files are kept as they are, to download again.'))),
    h('div', {},
      factTable,
      costTable ? [h('h3', {}, 'Speed and cost ', costNote), costTable,
        h('p', { class: 'hint' }, 'Leave the speed empty for the shop default. Rates are under Settings › Costs.')] : null,
      h('h3', {}, 'Customer and usage ', placeNote), placeBox,
      h('h3', {}, 'Tags ', tagNote), tagBox,
      h('h3', {}, 'Notes ', notesNote), notes,
      h('div', { style: 'margin-top:24px' }, remove)))];
}

// ---------- adding designs ----------

/** Every file in dropped folders, with its path ("Karate/Belts/x.dst"). */
async function droppedFiles(dt) {
  const out = [];
  const entries = [...dt.items].map((i) => i.webkitGetAsEntry && i.webkitGetAsEntry()).filter(Boolean);
  const walk = async (entry, path) => {
    if (entry.isFile) {
      const f = await new Promise((ok, bad) => entry.file(ok, bad));
      out.push({ file: f, path: path + f.name });
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      for (;;) {
        const batch = await new Promise((ok, bad) => reader.readEntries(ok, bad));
        if (!batch.length) break;
        for (const e of batch) await walk(e, path + entry.name + '/');
      }
    }
  };
  if (entries.length) { for (const e of entries) await walk(e, ''); }
  else for (const f of dt.files) out.push({ file: f, path: f.name });
  return out;
}

const stemOf = (name) => { const i = name.lastIndexOf('.'); return (i > 0 ? name.slice(0, i) : name).toLowerCase(); };
const folderOf = (path) => path.split('/').slice(0, -1);

function showUpload(initial) {
  let picked = initial.slice();
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  const tags = h('input', { type: 'text', placeholder: 'e.g. karate, back of jacket', 'aria-label': 'Tags for these designs' });
  const current = custById(state.customer);
  const custIn = h('input', { type: 'text', list: 'upload-customers', value: current ? custLabel(current) : '', placeholder: 'No customer', 'aria-label': 'Customer for these designs' });
  const usageSel = h('select', { 'aria-label': 'Usage for these designs' }, h('option', { value: '' }, '— from the folder names —'),
    state.usages.map((u) => h('option', { value: u.name, selected: u.name === state.usage }, u.name)));
  const list = h('div', { class: 'picked' });
  const out = h('div', {});
  const go = h('button', { class: 'btn primary', disabled: true }, 'Add to the library');
  const showPicked = () => {
    const visible = picked.filter((p) => !p.file.name.startsWith('.'));
    const designs = new Set(visible.map((p) => folderOf(p.path).join('/') + '|' + stemOf(p.file.name)));
    list.replaceChildren(visible.length
      ? h('div', {}, h('strong', {}, `${visible.length} files → about ${designs.size} designs`),
        h('div', { class: 'hint' }, visible.slice(0, 200).map((p) => p.path).join(', ') + (visible.length > 200 ? '…' : '')))
      : '');
    go.disabled = !visible.length;
  };
  const filesIn = h('input', { type: 'file', multiple: true, hidden: true, onchange: () => {
    picked = picked.concat([...filesIn.files].map((f) => ({ file: f, path: f.name })));
    showPicked();
  } });
  const folderIn = h('input', { type: 'file', multiple: true, hidden: true, webkitdirectory: true, onchange: () => {
    picked = picked.concat([...folderIn.files].map((f) => ({ file: f, path: f.webkitRelativePath || f.name })));
    showPicked();
  } });
  const drop = h('div', { class: 'drop' }, 'Drop files or folders here, or ',
    h('button', { class: 'link', onclick: () => filesIn.click() }, 'choose files'), ' / ',
    h('button', { class: 'link', onclick: () => folderIn.click() }, 'a folder'), '.');
  drop.addEventListener('dragover', (e) => { e.preventDefault(); e.stopPropagation(); drop.classList.add('over'); });
  drop.addEventListener('dragleave', () => drop.classList.remove('over'));
  drop.addEventListener('drop', async (e) => {
    e.preventDefault(); e.stopPropagation(); drop.classList.remove('over');
    picked = picked.concat(await droppedFiles(e.dataTransfer));
    showPicked();
  });

  go.addEventListener('click', async () => {
    const cv = custIn.value.trim();
    const cust = cv ? state.customers.find((c) => custLabel(c) === cv || c.name.toLowerCase() === cv.toLowerCase()) : null;
    if (cv && !cust) { out.replaceChildren(h('p', { class: 'error' }, `No customer "${cv}": create it first, or leave the field empty.`)); return; }
    go.disabled = true;
    const typed = tags.value.split(',').map((t) => t.trim()).filter(Boolean);
    // one request per folder, files of one design together; folder names (below the folder
    // that was picked) become tags, as with the import command
    const byFolder = new Map();
    for (const p of picked) {
      if (p.file.name.startsWith('.')) continue;
      const key = folderOf(p.path).join('/');
      if (!byFolder.has(key)) byFolder.set(key, []);
      byFolder.get(key).push(p);
    }
    let done = 0, designs = 0, added = 0;
    const skipped = [], notes = [];
    const total = [...byFolder.values()].reduce((n, l) => n + l.length, 0);
    for (const [folder, items] of byFolder) {
      const folderTags = folder ? folder.split('/').slice(1) : [];
      // batches of whole designs, about 50 files or 40 MB each
      const groups = new Map();
      for (const p of items) { const k = stemOf(p.file.name); if (!groups.has(k)) groups.set(k, []); groups.get(k).push(p); }
      let batch = [], size = 0;
      const send = async () => {
        if (!batch.length) return;
        const fd = new FormData();
        fd.append('tags', typed.concat(folderTags).join(','));
        if (cust) fd.append('customer_id', cust.id);
        if (usageSel.value) fd.append('usage', usageSel.value);
        for (const p of batch) fd.append('file', p.file, p.file.name);
        out.replaceChildren(h('p', {}, `Adding… ${done} of ${total} files`));
        try {
          const r = await api('POST', '/designs', fd);
          designs += r.designs.length; added += r.added.length; skipped.push(...r.skipped); notes.push(...r.warnings);
        } catch (e) { batch.forEach((p) => skipped.push({ name: p.path, reason: e.message })); }
        done += batch.length;
        batch = []; size = 0;
      };
      for (const g of groups.values()) {
        const gs = g.reduce((n, p) => n + p.file.size, 0);
        if (batch.length && (batch.length + g.length > 50 || size + gs > 40e6)) await send();
        batch.push(...g); size += gs;
      }
      await send();
    }
    out.replaceChildren(h('div', { class: 'report' },
      h('p', {}, h('strong', {}, `Added ${num(added)} files in ${num(designs)} designs.`)),
      skipped.length ? h('details', { open: skipped.length < 10 }, h('summary', {}, `${skipped.length} not added`),
        h('ul', {}, skipped.map((s) => h('li', { class: 'skip' }, `${s.name}: ${s.reason}`)))) : null,
      notes.length ? h('details', { open: notes.length < 10 }, h('summary', {}, `${notes.length} to check`),
        h('ul', {}, notes.map((s) => h('li', {}, `${s.name}: ${s.reason}`)))) : null));
    picked = [];
    showPicked();
    go.textContent = 'Add more';
    load();
  });

  const dlg = h('div', { class: 'dialog', role: 'dialog', 'aria-label': 'Add designs' },
    h('h2', {}, 'Add designs'),
    h('p', { class: 'hint' }, 'Files with the same name (KARATE.DST, KARATE.EMB, karate.jpg) become one design. Previews come from DST and EmbroideryCraft files; EMB and other files are kept to download again. Sub-folder names become tags.'),
    drop, filesIn, folderIn, list,
    h('div', { class: 'row', style: 'align-items:flex-end' },
      h('div', { style: 'flex:2; min-width:200px' }, h('label', {}, 'Customer (optional)'), custIn,
        h('datalist', { id: 'upload-customers' }, state.customers.map((c) => h('option', { value: custLabel(c) })))),
      h('div', { style: 'flex:1; min-width:140px' }, h('label', {}, 'Usage (optional)'), usageSel)),
    h('label', {}, 'Tags for all of them (optional, separated by commas)'), tags,
    out,
    h('div', { class: 'actions' }, h('button', { class: 'btn', onclick: close }, 'Close'), go));
  shade.addEventListener('click', close);
  document.body.append(shade, dlg);
  showPicked();
}

// drop anywhere on the page
let dragDepth = 0;
let overlay = null;
window.addEventListener('dragenter', (e) => {
  if (!state.me || document.querySelector('.dialog') || !e.dataTransfer.types.includes('Files')) return;
  dragDepth++;
  if (!overlay) { overlay = h('div', { class: 'dropzone-overlay' }, 'Drop to add to the library'); document.body.append(overlay); }
});
window.addEventListener('dragleave', () => {
  if (--dragDepth <= 0 && overlay) { overlay.remove(); overlay = null; dragDepth = 0; }
});
window.addEventListener('dragover', (e) => e.preventDefault());
window.addEventListener('drop', async (e) => {
  e.preventDefault();
  if (overlay) { overlay.remove(); overlay = null; }
  dragDepth = 0;
  if (!state.me || document.querySelector('.dialog')) return;
  const files = await droppedFiles(e.dataTransfer);
  if (files.length) showUpload(files);
});
window.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  const dlg = document.querySelector('.dialog');
  if (dlg) { dlg.remove(); document.querySelector('.dialog-shade')?.remove(); } else if (drawer) closeDesign();
});

// ---------- settings ----------

async function showSettings() {
  const shade = h('div', { class: 'dialog-shade' });
  const body = h('div', {});
  const close = () => { shade.remove(); dlg.remove(); };
  const dlg = h('div', { class: 'dialog', role: 'dialog', 'aria-label': 'Settings' },
    h('h2', {}, 'Settings'), body,
    h('div', { class: 'actions' }, h('button', { class: 'btn', onclick: close }, 'Close')));
  shade.addEventListener('click', close);
  document.body.append(shade, dlg);

  const render = async () => {
    const parts = [];
    // my password
    const old = h('input', { type: 'password', autocomplete: 'current-password' });
    const nw = h('input', { type: 'password', autocomplete: 'new-password' });
    const pmsg = h('div', {});
    parts.push(h('h3', {}, 'My password'),
      h('label', {}, 'Current password'), old,
      h('label', {}, 'New password (at least 8 characters)'), nw,
      h('div', { class: 'row', style: 'margin-top:8px' }, h('button', { class: 'btn small', onclick: async () => {
        try { await api('POST', '/me/password', { old: old.value, new: nw.value }); pmsg.replaceChildren(h('span', { class: 'saved' }, 'Changed. Sign in again.')); setTimeout(showSignIn, 1200); }
        catch (e) { pmsg.replaceChildren(h('span', { class: 'error' }, e.message)); }
      } }, 'Change password'), pmsg));

    if (state.me.admin) {
      const users = await api('GET', '/users');
      const uname = h('input', { type: 'text', autocomplete: 'off' });
      const upass = h('input', { type: 'password', autocomplete: 'new-password' });
      const uadmin = h('input', { type: 'checkbox' });
      const umsg = h('div', { class: 'error' });
      parts.push(h('h3', {}, 'People'),
        h('table', { class: 'list' }, users.map((u) => h('tr', {},
          h('td', {}, u.name, u.admin ? h('span', { class: 'hint' }, ' · administrator') : null),
          h('td', {}, u.id === state.me.id ? h('span', { class: 'hint' }, 'you') : armed('Remove', 'Remove?', async () => { await api('DELETE', '/users/' + u.id); render(); }))))),
        h('div', { class: 'row', style: 'margin-top:8px; align-items:flex-end' },
          h('div', { style: 'flex:1' }, h('label', {}, 'Name'), uname),
          h('div', { style: 'flex:1' }, h('label', {}, 'Password'), upass)),
        h('div', { class: 'row', style: 'margin-top:8px' },
          h('label', { style: 'margin:0; display:flex; gap:6px; align-items:center' }, uadmin, 'Administrator (manages people and API tokens)'),
          h('button', { class: 'btn small', onclick: async () => {
            try { await api('POST', '/users', { name: uname.value, password: upass.value, admin: uadmin.checked }); render(); }
            catch (e) { umsg.textContent = e.message; }
          } }, 'Add person')),
        umsg);

      const tokens = await api('GET', '/tokens');
      const tname = h('input', { type: 'text', placeholder: 'e.g. Odoo, machine station', autocomplete: 'off' });
      const tout = h('div', {});
      parts.push(h('h3', {}, 'API tokens'),
        h('p', { class: 'hint' }, 'For other programs (Odoo, scripts) to use this library through its API: they send the token as "Authorization: Bearer …". A token acts as the person who made it.'),
        tokens.length ? h('table', { class: 'list' }, tokens.map((t) => h('tr', {},
          h('td', {}, t.name, h('div', { class: 'hint' }, `by ${t.user} · ${t.last_used ? 'last used ' + new Date(t.last_used * 1000).toLocaleDateString() : 'never used'}`)),
          h('td', {}, armed('Revoke', 'Revoke?', async () => { await api('DELETE', '/tokens/' + t.id); render(); }))))) : null,
        h('div', { class: 'row', style: 'margin-top:8px' }, h('div', { style: 'flex:1' }, tname),
          h('button', { class: 'btn small', onclick: async () => {
            try {
              const r = await api('POST', '/tokens', { name: tname.value });
              await render();
              body.append(h('div', { style: 'margin-top:10px' },
                h('p', {}, h('strong', {}, 'Copy this token now: it is not shown again.')),
                h('div', { class: 'token' }, r.token),
                h('button', { class: 'btn small', style: 'margin-top:6px', onclick: () => navigator.clipboard?.writeText(r.token) }, 'Copy')));
            } catch (e) { tout.replaceChildren(h('span', { class: 'error' }, e.message)); }
          } }, 'Create token')),
        tout);
    }
    // machines
    if (state.me.admin) {
      const nc = h('input', { type: 'number', min: 1, max: 72, value: state.needleCount, style: 'max-width:90px' });
      nc.addEventListener('change', async () => {
        try { state.needleCount = (await api('PATCH', '/settings', { needle_count: Number(nc.value) })).needle_count; toast('Saved'); }
        catch (e) { toast(e.message); nc.value = state.needleCount; }
      });
      parts.push(h('h3', {}, 'Machines'), h('label', {}, 'Needles per head (for needle numbers in TBF files)'), nc);
    }

    // backups
    if (state.me.admin) {
      const box = h('div', {});
      const when = (t) => new Date(t * 1000).toLocaleString();
      const show = async (b) => {
        b = b || await api('GET', '/backup');
        const folder = h('input', { type: 'text', value: b.plan.folder, placeholder: 'e.g. /mnt/nas/Library backup or Z:\\Library backup' });
        const every = h('select', { style: 'width:auto' }, [[1, 'Every hour'], [4, 'Every 4 hours'], [12, 'Twice a day'], [24, 'Every day'], [168, 'Every week']]
          .map(([v, t]) => h('option', { value: v, selected: b.plan.every_hours === v }, t)));
        if (![1, 4, 12, 24, 168].includes(b.plan.every_hours)) every.append(h('option', { value: b.plan.every_hours, selected: true }, `Every ${b.plan.every_hours} hours`));
        const keep = h('input', { type: 'number', min: 1, max: 1000, value: b.plan.keep, style: 'max-width:90px' });
        const msg = h('div', {});
        const good = b.last_good;
        const last = b.last;
        let status;
        if (b.running) status = h('p', {}, 'Backing up now…');
        else if (!b.plan.folder) status = h('p', { class: 'error' }, 'No backup folder yet: the library is not backed up.');
        else if (last && !last.ok) status = h('p', { class: 'error' }, `The last backup failed (${when(last.at)}): ${last.message}`);
        else if (good) status = h('p', { class: 'saved' }, `Last backup ${when(good.at)}: ${good.files} design files (${good.copied} new), database ${good.database}.`);
        else status = h('p', { class: 'hint' }, 'Not backed up yet.');
        box.replaceChildren(
          h('p', { class: 'hint' }, `The server copies the database and every design file into this folder by itself. Choose a folder on another disk or on the NAS, not in the library folder (${b.library}). Files are only added there, so a design deleted by mistake can be restored; previews are drawn again after a restore.`),
          status,
          h('label', {}, 'Backup folder (on the computer running the library)'), folder,
          h('div', { class: 'row', style: 'margin-top:8px;gap:12px;flex-wrap:wrap;align-items:end' },
            h('div', {}, h('label', {}, 'How often'), every),
            h('div', {}, h('label', {}, 'Database copies kept'), keep)),
          h('div', { class: 'row', style: 'margin-top:8px' },
            h('button', { class: 'btn small', onclick: async () => {
              try { await show(await api('PATCH', '/backup', { folder: folder.value, every_hours: Number(every.value), keep: Number(keep.value) })); toast('Saved'); }
              catch (e) { msg.replaceChildren(h('span', { class: 'error' }, e.message)); }
            } }, 'Save'),
            h('button', { class: 'btn small', disabled: b.running || !b.plan.folder, onclick: async () => {
              try { await show(await api('POST', '/backup')); poll(); }
              catch (e) { msg.replaceChildren(h('span', { class: 'error' }, e.message)); }
            } }, 'Back up now'), msg),
          b.copies.length ? h('details', { style: 'margin-top:8px' }, h('summary', {}, `${b.copies.length} database copies in the folder`),
            h('ul', { class: 'hint' }, b.copies.slice().reverse().map((c) => h('li', {}, c)))) : null,
          h('details', { style: 'margin-top:8px' }, h('summary', {}, 'How to restore'),
            h('p', { class: 'hint' }, 'Stop the library, then on the computer that will run it:'),
            h('div', { class: 'token' }, `embcraft-server restore "${b.plan.folder || '<backup folder>'}" --data "<new library folder>"`),
            h('p', { class: 'hint' }, 'This takes the newest database copy; add --from with a name from the list above for an older one, or --list to see them. Then start the library with --data on the new folder. The same steps are in README.txt in the backup folder.')));
      };
      const poll = () => setTimeout(async () => {
        if (!box.isConnected) return;
        const b = await api('GET', '/backup');
        await show(b);
        if (b.running) poll();
      }, 2000);
      parts.push(h('h3', {}, 'Backups'), box);
      await show();
    }

    // costs
    const st = await api('GET', '/settings');
    const c = st.costing;
    const fields = [
      ['default_speed', 'Default speed (stitches a minute)', 10], ['heads', 'Heads sewing at once', 1],
      ['colour_change_seconds', 'Seconds per colour change', 1], ['trim_seconds', 'Seconds per trim', 1],
      ['handling_seconds', 'Handling per piece, seconds (hooping, unhooping, clean-up)', 5],
      ['hourly_rate', 'Machine and operator, per hour', 0.5], ['thread_per_1000', 'Thread and bobbin, per 1000 stitches', 0.01],
      ['backing_per_piece', 'Backing and topping, per piece', 0.01], ['price_per_1000', 'Selling price per 1000 stitches', 0.05],
      ['minimum_price', 'Minimum price per piece', 0.5]];
    const ins = {};
    const cmsg = h('div', {});
    const grid = h('div', { class: 'cost-grid' }, fields.map(([k, label, step]) => {
      ins[k] = h('input', { type: 'number', min: 0, step, value: c[k], disabled: !state.me.admin });
      return h('div', {}, h('label', {}, label), ins[k]);
    }));
    ins.currency = h('input', { type: 'text', value: c.currency, maxlength: 6, disabled: !state.me.admin });
    grid.append(h('div', {}, h('label', {}, 'Currency sign'), ins.currency));
    parts.push(h('h3', {}, 'Costs'),
      h('p', { class: 'hint' }, 'Used for the time and cost of each design: sewing time = stitches ÷ speed + colour changes + trims; per piece, the run is shared by the heads and handling is added; cost = time × hourly rate + thread + backing; price = stitches × price per 1000, at least the minimum.'),
      grid,
      state.me.admin ? h('div', { class: 'row', style: 'margin-top:8px' }, h('button', { class: 'btn small', onclick: async () => {
        const body = { currency: ins.currency.value.trim() || '$' };
        for (const [k] of fields) body[k] = Number(ins[k].value);
        try { await api('PATCH', '/settings', { costing: body }); cmsg.replaceChildren(h('span', { class: 'saved' }, 'Saved')); load(); }
        catch (e) { cmsg.replaceChildren(h('span', { class: 'error' }, e.message)); }
      } }, 'Save costs'), cmsg) : h('p', { class: 'hint' }, 'Only an administrator can change them.'));

    // usages
    const usages = await api('GET', '/usages');
    const uName = h('input', { type: 'text', placeholder: 'e.g. Kimono back' });
    const uWords = h('input', { type: 'text', placeholder: 'folder words, e.g. dos kimono' });
    const uNames = h('input', { type: 'checkbox' });
    const uMsg = h('div', { class: 'error' });
    const wordsIn = (u) => {
      const el = h('input', { type: 'text', value: u.words, 'aria-label': `Folder words for ${u.name}` });
      el.addEventListener('change', async () => { try { await api('PATCH', '/usages/' + u.id, { words: el.value }); } catch (e) { toast(e.message); } });
      return el;
    };
    const namesIn = (u) => {
      const el = h('input', { type: 'checkbox', checked: u.names, title: 'Designs for this usage are names', 'aria-label': `${u.name}: names` });
      el.addEventListener('change', async () => { await api('PATCH', '/usages/' + u.id, { names: el.checked }); });
      return el;
    };
    parts.push(h('h3', {}, 'Usages'),
      h('p', { class: 'hint' }, 'What designs are sewn on. When importing folders, a folder named with one of the words (lower case, no accents) gives its designs that usage; ticked usages make them names rather than artwork.'),
      h('table', { class: 'list' }, h('tr', {}, h('th', {}, 'Usage'), h('th', {}, 'Folder words'), h('th', {}, 'Names'), h('th', {})),
        usages.map((u) => h('tr', {}, h('td', {}, u.name, h('div', { class: 'hint' }, `${num(u.designs)} designs`)), h('td', {}, wordsIn(u)), h('td', {}, namesIn(u)),
          h('td', {}, state.me.admin ? armed('Remove', 'Remove?', async () => { await api('DELETE', '/usages/' + u.id); render(); }) : null)))),
      h('div', { class: 'row', style: 'margin-top:8px; align-items:center' },
        h('div', { style: 'flex:1' }, uName), h('div', { style: 'flex:2' }, uWords),
        h('label', { class: 'check' }, uNames, 'Names'),
        h('button', { class: 'btn small', onclick: async () => {
          try { await api('POST', '/usages', { name: uName.value, words: uWords.value, names: uNames.checked }); render(); }
          catch (e) { uMsg.textContent = e.message; }
        } }, 'Add usage')),
      uMsg);

    parts.push(h('h3', {}, 'About'), h('p', { class: 'hint' }, 'EmbroideryCraft Library. API: ', h('code', {}, location.origin + '/api/v1')));
    body.replaceChildren(...parts);
  };
  render();
}

// ---------- choosing a thread for one colour of a design ----------

async function inUseCharts() {
  try { return (await api('GET', '/thread-charts')).filter((c) => c.in_use); } catch { return []; }
}

async function pickThread(d, i, cw = null, preview = null) {
  const list0 = cw ? cw.threads : d.threads;
  threadDialog({
    title: `Colour ${i + 1} of ${list0.length}${cw ? ' · ' + cw.name : ''}`,
    current: list0[i],
    preview,
    background: cw ? cw.background : d.background,
    onpick: async (t) => {
      const list = list0.map((x) => ({ ...x }));
      list[i] = t;
      if (cw) await api('PATCH', '/colourways/' + cw.id, { threads: list });
      else await change(d, { threads: list });
      openDesign(d.id);
    },
  });
}

/** Choose a thread from the charts in use (or a plain colour); onpick(thread) saves it. */
async function threadDialog({ title, current, onpick, preview = null, background = '' }) {
  current = current && current.rgb ? current : { rgb: '#808080', code: '', name: '', chart: '' };
  const charts = await inUseCharts();
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  shade.addEventListener('click', close);

  const chartSel = h('select', { 'aria-label': 'Chart' },
    h('option', { value: '' }, 'All charts in use'),
    charts.map((c) => h('option', { value: c.id, selected: c.default }, c.name)));
  const search = h('input', { type: 'search', placeholder: 'Number or name…', 'aria-label': 'Search threads' });
  const favs = h('input', { type: 'checkbox' });
  const near = h('input', { type: 'checkbox' });
  const results = h('div', { class: 'tgrid' });
  const count = h('div', { class: 'hint' });

  const assign = async (t) => {
    try { await onpick(t); } catch (e) { toast(e.message); return; }
    close();
  };

  const refresh = async () => {
    const p = new URLSearchParams({ q: search.value.trim() });
    if (chartSel.value) p.set('chart', chartSel.value);
    if (favs.checked) p.set('favourites', 'true');
    if (near.checked) p.set('near', current.rgb);
    let list;
    try { list = await api('GET', '/threads?' + p); } catch (e) { results.textContent = e.message; return; }
    // favourites first, unless sorted by closeness
    if (!near.checked) list.sort((a, b) => (b.favourite - a.favourite));
    count.textContent = `${list.length} thread${list.length === 1 ? '' : 's'}`;
    results.replaceChildren(...list.map((t) => tile(t, {
      onpick: () => assign({ rgb: t.rgb, code: t.code, name: t.name, chart: t.chart }),
      selected: current.chart === t.chart && current.code === t.code,
      extra: t.delta !== undefined ? `ΔE ${t.delta}` : null,
    })));
    if (!list.length) results.append(h('p', { class: 'hint' }, charts.length ? 'No thread matches.' : 'No thread chart is in use: see the Threads page.'));
  };
  const later = debounce(refresh, 200);
  search.addEventListener('input', later);
  [chartSel, favs, near].forEach((el) => el.addEventListener('change', refresh));

  const plain = h('input', { type: 'color', value: current.rgb, 'aria-label': 'Plain colour' });
  const dlg = h('div', { class: 'dialog wide', role: 'dialog', 'aria-label': title },
    h('div', { class: 'row' },
      preview ? h('div', { class: 'pick-preview', style: fabricStyle(background) }, h('img', { src: preview, alt: 'This colour in the design' })) : null,
      h('span', { class: 'sw big', style: `background:${current.rgb}` }),
      h('h2', {}, title),
      current.chart ? h('span', { class: 'hint' }, `now ${current.chart} ${current.code}`) : null),
    h('div', { class: 'row picker-bar' }, chartSel, h('div', { style: 'flex:1; min-width:160px' }, search),
      h('label', { class: 'check' }, favs, 'Favourites only'),
      h('label', { class: 'check' }, near, 'Closest to the current colour')),
    count, results,
    h('div', { class: 'actions', style: 'justify-content:space-between' },
      h('div', { class: 'row' }, plain,
        h('button', { class: 'btn small', onclick: () => assign({ rgb: plain.value, code: '', name: '', chart: '' }) }, 'Use a plain colour')),
      h('button', { class: 'btn', onclick: close }, 'Close')));
  document.body.append(shade, dlg);
  search.focus();
  refresh();
}

/** One thread as a swatch tile. */
function tile(t, { onpick, onstar, selected, extra }) {
  const star = onstar
    ? h('button', { class: 'star' + (t.favourite ? ' on' : ''), title: t.favourite ? 'Remove from favourites' : 'Add to favourites',
      'aria-label': (t.favourite ? 'Remove from favourites ' : 'Add to favourites ') + t.code,
      onclick: (e) => { e.stopPropagation(); onstar(); } }, t.favourite ? '★' : '☆')
    : (t.favourite ? h('span', { class: 'star on', 'aria-hidden': 'true' }, '★') : null);
  return h('div', { class: 'tile' + (selected ? ' selected' : ''), role: 'button', tabindex: 0, title: `${t.chart} ${t.code}${t.name ? ' · ' + t.name : ''}`,
    onclick: onpick, onkeydown: (e) => { if (e.key === 'Enter') onpick(); } },
    h('span', { class: 'chip-colour', style: `background:${t.rgb}` }), star,
    h('span', { class: 'code' }, t.code),
    h('span', { class: 'tname' }, t.name || (extra || '')),
    t.name && extra ? h('span', { class: 'tname' }, extra) : null);
}

// ---------- the Threads page: charts, favourites, the shop's own threads ----------

const thr = { charts: [], chart: null, q: '', favs: false };

async function showThreads(page) {
  const side = h('aside', { class: 'side' });
  const main = h('main', { class: 'main' });
  page.replaceChildren(side, main);

  const loadCharts = async () => {
    thr.charts = await api('GET', '/thread-charts');
    if (!thr.charts.some((c) => c.id === thr.chart)) thr.chart = (thr.charts.find((c) => c.default) || thr.charts[0] || {}).id ?? null;
  };

  const renderSide = () => {
    const nameIn = h('input', { type: 'text', placeholder: 'e.g. Madeira Polyneon 40, Our stock', 'aria-label': 'New chart name' });
    side.replaceChildren(h('h3', {}, 'Thread charts'),
      h('div', { class: 'tag-list' }, ...thr.charts.map((c) => h('button', {
        class: 'tag-item' + (c.id === thr.chart ? ' on' : ''), onclick: () => { thr.chart = c.id; renderSide(); renderMain(); } },
        h('span', {}, c.name, c.default ? h('span', { class: 'badge' }, 'default') : null, !c.in_use ? h('span', { class: 'badge off' }, 'hidden') : null),
        h('span', { class: 'n' }, num(c.count))))),
      h('h3', {}, 'New chart'),
      nameIn,
      h('button', { class: 'btn small', style: 'margin-top:6px', onclick: async () => {
        try { const c = await api('POST', '/thread-charts', { name: nameIn.value }); thr.chart = c.id; await loadCharts(); renderSide(); renderMain(); }
        catch (e) { toast(e.message); }
      } }, '+ Create'));
  };

  const grid = h('div', { class: 'tgrid' });
  const countLine = h('span', {});
  const loadThreads = async () => {
    if (!thr.chart) { grid.replaceChildren(); return; }
    const p = new URLSearchParams({ chart: thr.chart, q: thr.q });
    if (thr.favs) p.set('favourites', 'true');
    const list = await api('GET', '/threads?' + p);
    countLine.textContent = `${num(list.length)} thread${list.length === 1 ? '' : 's'}`;
    grid.replaceChildren(...list.map((t) => tile(t, {
      onpick: () => editThread(t, refreshAll),
      onstar: async () => { await api('PATCH', '/threads/' + t.id, { favourite: !t.favourite }); refreshAll(); },
    })));
    if (!list.length) grid.append(h('p', { class: 'hint' }, thr.q || thr.favs ? 'No thread matches.' : 'This chart is empty: add threads one by one, or import a list.'));
  };
  const refreshAll = async () => { await loadCharts(); renderSide(); await loadThreads(); };

  const renderMain = () => {
    const c = thr.charts.find((x) => x.id === thr.chart);
    if (!c) { main.replaceChildren(h('div', { class: 'empty' }, h('div', { class: 'big' }, 'No thread chart'), 'Create one on the left.')); return; }
    const field = (label, value, key, ph) => {
      const el = h('input', { type: 'text', value, placeholder: ph || '' });
      el.addEventListener('change', async () => {
        try { await api('PATCH', '/thread-charts/' + c.id, { [key]: el.value }); await loadCharts(); renderSide(); }
        catch (e) { toast(e.message); el.value = value; }
      });
      return h('div', { style: 'flex:1; min-width:180px' }, h('label', {}, label), el);
    };
    const inUse = h('input', { type: 'checkbox', checked: c.in_use });
    inUse.addEventListener('change', async () => { await api('PATCH', '/thread-charts/' + c.id, { in_use: inUse.checked }); refreshAll(); });
    const search = h('input', { type: 'search', placeholder: 'Number or name…', value: thr.q, 'aria-label': 'Search this chart' });
    search.addEventListener('input', debounce(() => { thr.q = search.value.trim(); loadThreads(); }, 200));
    const favs = h('input', { type: 'checkbox', checked: thr.favs });
    favs.addEventListener('change', () => { thr.favs = favs.checked; loadThreads(); });
    main.replaceChildren(
      h('div', { class: 'row', style: 'align-items:flex-end' },
        field('Chart name', c.name, 'name'), field('Brand', c.brand, 'brand', 'e.g. AMANN Isacord')),
      c.notes ? h('p', { class: 'hint' }, c.notes) : null,
      h('div', { class: 'row', style: 'margin:12px 0' },
        h('label', { class: 'check' }, inUse, 'In use (offered when choosing threads)'),
        c.default ? h('span', { class: 'badge' }, 'Shop default') :
          h('button', { class: 'btn small', onclick: async () => { await api('PATCH', '/thread-charts/' + c.id, { default: true }); refreshAll(); renderMain(); } }, 'Make it the shop default'),
        h('div', { class: 'spacer', style: 'flex:1' }),
        h('button', { class: 'btn small', onclick: () => editThread({ chart_id: c.id, code: '', name: '', rgb: '#808080' }, refreshAll) }, '+ Add thread'),
        h('button', { class: 'btn small', onclick: () => importList(c, refreshAll) }, 'Import a list…'),
        h('a', { class: 'btn small', href: `/api/v1/thread-charts/${c.id}/export`, download: `${c.name}.csv` }, 'Export CSV'),
        !c.builtin && state.me.admin ? armed('Delete chart', `Delete "${c.name}"?`, async () => {
          try { await api('DELETE', '/thread-charts/' + c.id); } catch (e) { toast(e.message); return; }
          thr.chart = null; await refreshAll(); renderMain();
        }) : null),
      h('div', { class: 'row', style: 'margin-bottom:12px' }, h('div', { style: 'flex:1; max-width:340px' }, search),
        h('label', { class: 'check' }, favs, 'Favourites only'), countLine,
        h('span', { class: 'hint' }, '☆ marks a favourite: favourites come first when choosing a thread.')),
      grid);
    loadThreads();
  };

  try { await loadCharts(); } catch (e) { main.textContent = e.message; return; }
  renderSide();
  renderMain();
}

/** Add or change one thread of a chart. */
function editThread(t, done) {
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  shade.addEventListener('click', close);
  const code = h('input', { type: 'text', value: t.code, placeholder: 'e.g. 1902' });
  const name = h('input', { type: 'text', value: t.name, placeholder: 'e.g. Dojo red (optional)' });
  const colour = h('input', { type: 'color', value: t.rgb });
  const err = h('div', { class: 'error' });
  const save = async () => {
    try {
      if (t.id) await api('PATCH', '/threads/' + t.id, { code: code.value, name: name.value, rgb: colour.value });
      else await api('POST', '/threads', { chart_id: t.chart_id, code: code.value, name: name.value, rgb: colour.value });
      close(); done();
    } catch (e) { err.textContent = e.message; }
  };
  const dlg = h('div', { class: 'dialog', role: 'dialog', 'aria-label': t.id ? 'Thread' : 'New thread' },
    h('h2', {}, t.id ? `${t.chart} ${t.code}` : 'New thread'),
    h('label', {}, 'Number'), code,
    h('label', {}, 'Name'), name,
    h('label', {}, 'Colour (as close to the real thread as you can)'), colour,
    err,
    h('div', { class: 'actions', style: 'justify-content:space-between' },
      t.id ? armed('Remove thread', 'Remove it from the chart?', async () => { await api('DELETE', '/threads/' + t.id); close(); done(); }) : h('span', {}),
      h('div', { class: 'row' }, h('button', { class: 'btn', onclick: close }, 'Cancel'), h('button', { class: 'btn primary', onclick: save }, 'Save'))));
  document.body.append(shade, dlg);
  code.focus();
}

/** Import threads into a chart from a CSV file or pasted text. */
function importList(c, done) {
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  shade.addEventListener('click', close);
  const text = h('textarea', { rows: 9, placeholder: '1902,Dojo red,#af0827\n3900,,#0065ab' });
  const file = h('input', { type: 'file', accept: '.csv,.txt,text/csv,text/plain', onchange: async () => { if (file.files[0]) text.value = await file.files[0].text(); } });
  const out = h('div', {});
  const dlg = h('div', { class: 'dialog', role: 'dialog', 'aria-label': 'Import threads' },
    h('h2', {}, `Import into ${c.name}`),
    h('p', { class: 'hint' }, 'One thread per line: its number, its colour as #rrggbb and, if you like, a name, separated by commas, semicolons or tabs (a spreadsheet saved as CSV works). Threads already in the chart get the new colour and name.'),
    file, text, out,
    h('div', { class: 'actions' }, h('button', { class: 'btn', onclick: close }, 'Close'),
      h('button', { class: 'btn primary', onclick: async () => {
        try {
          const r = await api('POST', `/thread-charts/${c.id}/import`, { csv: text.value });
          out.replaceChildren(h('p', {}, h('strong', {}, `${r.added} added, ${r.updated} updated.`)),
            r.errors.length ? h('ul', {}, r.errors.slice(0, 50).map((e) => h('li', { class: 'skip' }, e))) : null);
          done();
        } catch (e) { out.replaceChildren(h('p', { class: 'error' }, e.message)); }
      } }, 'Import')));
  document.body.append(shade, dlg);
}

boot();

// ---------- machines and their design sticks ----------

async function loadMachines() {
  try { state.machines = await api('GET', '/machines'); } catch { state.machines = []; }
  return state.machines;
}

const machineLine = (m) => [m.brand, m.model, m.type_code && `type ${m.type_code}`, m.serial && `no. ${m.serial}`].filter(Boolean).join(' · ');
const LAST_MACHINE = 'ec-last-machine';
function lastMachine() {
  let id = null;
  try { id = Number(localStorage.getItem(LAST_MACHINE)); } catch { /* private window */ }
  return state.machines.find((m) => m.id === id) || state.machines.find((m) => m.stick) || state.machines[0] || null;
}
function rememberMachine(id) { try { localStorage.setItem(LAST_MACHINE, String(id)); } catch { /* private window */ } }

async function showMachines(page) {
  const main = h('main', { class: 'main machines' });
  page.replaceChildren(main);
  await loadMachines();
  const admin = state.me.admin;
  const head = h('div', { class: 'row', style: 'justify-content:space-between; margin-bottom:12px' },
    h('h2', { style: 'margin:0' }, 'Machines'),
    admin ? h('div', { class: 'row' },
      h('button', { class: 'btn small', onclick: () => prepareStick(() => showMachines(page)) }, 'Prepare a design stick'),
      h('button', { class: 'btn small', onclick: showStickKey }, 'Library key for the sticks'),
      h('button', { class: 'btn primary small', onclick: () => editMachine(null, () => showMachines(page)) }, '+ Add machine')) : null);
  if (!state.machines.length) {
    main.replaceChildren(head, h('p', { class: 'hint' }, admin
      ? 'No machine yet. Add each machine with its needles, and the design stick (the Raspberry Pi in its USB port) to send designs to it from here.'
      : 'No machine yet: an administrator adds them.'));
    return;
  }
  main.replaceChildren(head, h('div', { class: 'mgrid' }, state.machines.map((m) => machineCard(m, () => showMachines(page)))));
}

function needleStrip(m) {
  return h('div', { class: 'needles', 'aria-label': 'Needle threads' }, Array.from({ length: m.needles }, (_, i) => {
    const t = m.threads[i];
    const set = t && t.rgb;
    return h('span', { class: 'nd' + (set ? '' : ' unset'), title: `Needle ${i + 1}: ${set ? (t.chart ? `${t.chart} ${t.code}${t.name ? ' · ' + t.name : ''}` : t.rgb) : 'not set'}` },
      h('span', { class: 'nd-sw', style: set ? `background:${t.rgb}` : '' }), h('span', { class: 'nd-n' }, i + 1));
  }));
}

function machineCard(m, refresh) {
  const box = h('div', { class: 'stick' });
  const card = h('section', { class: 'mcard' },
    h('div', { class: 'row', style: 'justify-content:space-between' },
      h('div', {}, h('h3', { style: 'margin:0' }, m.name), h('div', { class: 'hint' }, machineLine(m) || '—')),
      state.me.admin ? h('button', { class: 'btn small', onclick: () => editMachine(m, refresh) }, 'Edit') : null),
    h('div', { class: 'hint' }, `${m.needles} needles · sends ${m.format.toUpperCase()}${m.format === 'tbf' ? ' (with needles)' : ''}`),
    needleStrip(m),
    m.notes ? h('p', { class: 'hint' }, m.notes) : null,
    box);
  showStick(m, box);
  return card;
}

/** What is on the machine's design stick, with buttons to manage it. */
async function showStick(m, box) {
  if (!m.stick) {
    box.replaceChildren(h('p', { class: 'hint' }, 'No design stick linked: designs are carried on a USB stick. To send designs from the library, Edit the machine and fill in its design stick address (e.g. shop@ecstick1.local).'));
    return;
  }
  const title = h('div', { class: 'row', style: 'justify-content:space-between' },
    h('strong', {}, 'Design stick ', h('span', { class: 'hint' }, m.stick)),
    h('button', { class: 'btn small', onclick: () => showStick(m, box) }, 'Refresh'));
  box.replaceChildren(title, h('p', { class: 'hint' }, 'Asking the stick…'));
  let s;
  try { s = await api('GET', `/machines/${m.id}/stick`); } catch (e) {
    box.replaceChildren(title, h('p', { class: 'error' }, e.message));
    return;
  }
  renderStick(m, box, title, s);
}

function renderStick(m, box, title, s) {
  const act = async (path, body, done) => {
    try { renderStick(m, box, title, await api('POST', `/machines/${m.id}/stick${path}`, body)); if (done) toast(done); }
    catch (e) { toast(e.message); }
  };
  box.replaceChildren(title,
    h('div', { class: 'hint' }, h('span', { class: 'led' + (s.attached ? ' on' : '') }), s.attached ? 'The machine is using it' : 'Not plugged into a machine that is on',
      s.settings ? ` · ${s.settings}` : ''),
    s.designs.length
      ? h('ul', { class: 'on-stick' }, s.designs.map((n) => h('li', {}, h('span', {}, n),
        armed('Remove', 'Remove?', () => act('/remove', { names: [n] }, `Removed ${n}`), 'btn small'))))
      : h('p', { class: 'hint' }, 'The stick is empty.'),
    s.designs.length ? h('div', { class: 'row' }, armed('Empty the stick', 'Remove every design?', () => act('/clear', {}, 'The stick is empty.'))) : null,
    h('p', { class: 'hint' }, 'Send designs from a design (Send to machine). The machine sees the change once you go back into its USB menu.'));
}

function editMachine(m, done) {
  const isNew = !m || !m.id;
  m = { name: '', brand: 'Tajima', model: '', type_code: '', serial: '', needles: 15, format: 'dst', threads: [], stick: '', stick_via: '', notes: '', ...(m || {}) };
  let threads = m.threads.map((t) => ({ ...t }));
  const f = (key, label, attrs = {}) => {
    const el = h('input', { type: 'text', value: m[key], ...attrs });
    return [h('label', {}, label), el, el];
  };
  const [lName, name] = f('name', 'Name (as people call it)', { placeholder: 'e.g. Tajima 1, Left machine' });
  const [lBrand, brand] = f('brand', 'Brand');
  const [lModel, model] = f('model', 'Model', { placeholder: 'e.g. TFMX-C1501' });
  const [lType, type] = f('type_code', 'Type (on the maker\'s plate)', { placeholder: 'e.g. 1-1AU' });
  const [lSerial, serial] = f('serial', 'Serial number (MFG. NO.)');
  const needles = h('input', { type: 'number', min: 1, max: 72, value: m.needles, style: 'max-width:90px' });
  const format = h('select', {}, [['dst', 'DST (the machine keeps its own needles)'], ['tbf', 'TBF (Tajima, carries the needles)']]
    .map(([v, l]) => h('option', { value: v, selected: v === m.format }, l)));
  const [lStick, stickIn] = f('stick', 'Design stick (Raspberry Pi) address', { placeholder: 'e.g. shop@ecstick1.local — empty without one' });
  const [lVia, via] = f('stick_via', 'Reach it through (only if the stick is on another network)', { placeholder: 'e.g. me@10.0.0.20' });
  const notes = h('textarea', { rows: 2 }, m.notes);
  const err = h('div', { class: 'error', role: 'alert' });

  const grid = h('div', { class: 'needle-edit' });
  const drawNeedles = () => {
    const n = Math.max(1, Math.min(72, Number(needles.value) || 1));
    grid.replaceChildren(...Array.from({ length: n }, (_, i) => {
      const t = threads[i];
      const set = t && t.rgb;
      const pick = () => threadDialog({ title: `Needle ${i + 1}`, current: t, onpick: async (nt) => {
        while (threads.length <= i) threads.push({ rgb: '', code: '', name: '', chart: '' });
        threads[i] = nt;
        drawNeedles();
      } });
      return h('div', { class: 'nrow' }, h('span', { class: 'num' }, i + 1),
        h('button', { class: 'swatch-btn', style: set ? `background:${t.rgb}` : '', title: 'Choose the thread', 'aria-label': `Thread on needle ${i + 1}`, onclick: pick }),
        h('button', { class: 'thread-label', onclick: pick }, set
          ? (t.chart ? [h('strong', {}, t.code), h('span', { class: 'hint' }, ` ${t.chart}${t.name ? ' · ' + t.name : ''}`)] : h('span', {}, t.rgb))
          : h('span', { class: 'hint' }, 'Not set')),
        set ? h('button', { class: 'link', onclick: () => { threads[i] = { rgb: '', code: '', name: '', chart: '' }; drawNeedles(); } }, 'Clear') : null);
    }));
  };
  needles.addEventListener('change', drawNeedles);
  drawNeedles();

  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  const save = async () => {
    const body = { name: name.value, brand: brand.value, model: model.value, type_code: type.value, serial: serial.value,
      needles: Number(needles.value), format: format.value, threads, stick: stickIn.value, stick_via: via.value, notes: notes.value };
    try {
      if (isNew) await api('POST', '/machines', body); else await api('PATCH', '/machines/' + m.id, body);
    } catch (e) { err.textContent = e.message; return; }
    close();
    await loadMachines();
    state.needleCount = (await api('GET', '/settings')).needle_count;
    done();
  };
  const dlg = h('div', { class: 'dialog wide', role: 'dialog', 'aria-label': isNew ? 'New machine' : 'Edit ' + m.name },
    h('h2', {}, isNew ? 'New machine' : m.name),
    h('div', { class: 'two-cols' },
      h('div', {}, lName, name, lBrand, brand, lModel, model, lType, type, lSerial, serial,
        h('label', {}, 'Needles'), needles, h('label', {}, 'File it takes'), format,
        lStick, stickIn, lVia, via, h('label', {}, 'Notes'), notes),
      h('div', {}, h('label', {}, 'Thread usually on each needle'),
        h('p', { class: 'hint' }, 'Used to choose needles for a design: the needle with the same thread, else the closest colour.'), grid)),
    err,
    h('div', { class: 'actions', style: 'justify-content:space-between' },
      isNew ? h('span', {}) : armed('Delete machine', `Delete ${m.name}?`, async () => {
        try { await api('DELETE', '/machines/' + m.id); } catch (e) { toast(e.message); return; }
        close(); await loadMachines(); done();
      }),
      h('div', { class: 'row' }, h('button', { class: 'btn', onclick: close }, 'Cancel'), h('button', { class: 'btn primary', onclick: save }, 'Save'))));
  shade.addEventListener('click', close);
  document.body.append(shade, dlg);
  name.focus();
}

/** The library's ssh key, and how to add it to a design stick. */
async function showStickKey() {
  let key;
  try { key = (await api('GET', '/machines/key')).key; } catch (e) { toast(e.message); return; }
  const target = (state.machines.find((m) => m.stick) || {}).stick || 'shop@ecstick1.local';
  const cmd = `ssh ${target} "mkdir -p ~/.ssh && echo '${key}' >> ~/.ssh/authorized_keys"`;
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  const dlg = h('div', { class: 'dialog wide', role: 'dialog', 'aria-label': 'Library key' },
    h('h2', {}, 'Let the library reach a design stick'),
    h('p', {}, 'Once per stick, from a computer that can already sign in to it, run this (it asks for the stick\'s password):'),
    h('pre', { class: 'code' }, cmd),
    h('button', { class: 'btn small', onclick: () => { navigator.clipboard?.writeText(cmd); toast('Copied'); } }, 'Copy'),
    h('p', { class: 'hint' }, 'For another stick, change the address after ssh. The key itself:'),
    h('pre', { class: 'code' }, key),
    h('div', { class: 'actions' }, h('button', { class: 'btn', onclick: close }, 'Close')));
  shade.addEventListener('click', close);
  document.body.append(shade, dlg);
}

/** A new design stick: the files for its card, then waiting for it to answer and giving it a machine. */
async function prepareStick(done) {
  let start;
  try { start = await api('GET', '/machines/stick-card'); } catch (e) { toast(e.message); return; }
  let zone = 'America/Toronto';
  try { zone = Intl.DateTimeFormat().resolvedOptions().timeZone || zone; } catch { /* old browser */ }
  const shade = h('div', { class: 'dialog-shade' });
  let timer = null;
  const close = () => { clearTimeout(timer); timer = null; shade.remove(); dlg.remove(); };
  const dlg = h('div', { class: 'dialog wide', role: 'dialog', 'aria-label': 'Prepare a design stick' });
  shade.addEventListener('click', close);
  document.body.append(shade, dlg);

  // ---- step 1: what the card needs
  const name = h('input', { type: 'text', value: start.name, spellcheck: 'false', autocomplete: 'off', style: 'max-width:220px' });
  const ssid = h('input', { type: 'text', autocomplete: 'off', spellcheck: 'false', placeholder: 'e.g. ShopWiFi', 'data-1p-ignore': 'true', 'data-lpignore': 'true', 'data-bwignore': 'true' });
  const pw = h('input', { type: 'text', class: 'secret', autocomplete: 'off', spellcheck: 'false', name: 'ec-stick-wifi',
    'data-1p-ignore': 'true', 'data-lpignore': 'true', 'data-bwignore': 'true', 'data-form-type': 'other', placeholder: 'The Wi-Fi password' });
  const show = h('label', { class: 'check' }, h('input', { type: 'checkbox', onchange: (e) => pw.classList.toggle('secret', !e.target.checked) }), ' Show');
  const country = h('input', { type: 'text', value: 'CA', maxlength: 2, style: 'max-width:70px; text-transform:uppercase' });
  const tz = h('input', { type: 'text', value: zone, style: 'max-width:260px' });
  const keys = h('textarea', { rows: 3, spellcheck: 'false', placeholder: 'ssh-ed25519 AAAA… me@my-computer' });
  const err = h('div', { class: 'error', role: 'alert' });
  const go = h('button', { class: 'btn primary' }, 'Download the card files');
  const reachText = () => `It will be reached as ${start.user}@${name.value.trim().toLowerCase() || '…'}.local. Each stick needs its own name.`;
  const reach = h('p', { class: 'hint' }, reachText());

  const download = async () => {
    err.textContent = '';
    const body = { name: name.value.trim().toLowerCase(), ssid: ssid.value, wifi_password: pw.value,
      country: country.value.trim().toUpperCase(), timezone: tz.value.trim(), keys: keys.value, user: start.user };
    go.disabled = true;
    let blob;
    try {
      const res = await fetch('/api/v1/machines/stick-card', { method: 'POST', headers: { 'X-EC': '1', 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
      if (!res.ok) {
        let m = res.statusText;
        try { m = (await res.json()).error || m; } catch { /* not JSON */ }
        throw new Error(m);
      }
      blob = await res.blob();
    } catch (e) { err.textContent = e.message; go.disabled = false; return; }
    const url = URL.createObjectURL(blob);
    const a = h('a', { href: url, download: `${body.name}-card.zip` });
    document.body.append(a); a.click(); a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 60000);
    pw.value = '';
    waiting(body.name);
  };
  go.addEventListener('click', download);

  dlg.replaceChildren(
    h('h2', {}, 'Prepare a design stick'),
    h('p', {}, 'The Library makes the files for a new Raspberry Pi\'s card: it joins the Wi-Fi, takes its name, lets the Library in, and installs the stick by itself on its first start.'),
    h('label', {}, 'Name of the stick'), name,
    reach,
    h('label', {}, 'Wi-Fi at the machines (2.4 GHz)'), ssid,
    h('label', {}, 'Wi-Fi password'), h('div', { class: 'row' }, pw, show),
    h('p', { class: 'hint' }, 'Only put in the downloaded files, never kept by the Library. The Pi takes it off the card once it is set up.'),
    h('details', {},
      h('summary', {}, 'More settings'),
      h('label', {}, 'Country (for the Wi-Fi)'), country,
      h('label', {}, 'Time zone'), tz,
      h('label', {}, 'Other computers allowed in (their public ssh keys, one per line)'), keys,
      h('p', { class: 'hint' }, 'The Library is always allowed in. There is no password: the stick only lets in these keys.')),
    err,
    h('div', { class: 'actions' }, h('button', { class: 'btn', onclick: close }, 'Cancel'), go));
  name.addEventListener('input', () => { reach.textContent = reachText(); });
  ssid.focus();

  // ---- step 2: write the card, then wait for the stick
  function waiting(stickName) {
    const target = h('input', { type: 'text', value: `${start.user}@${stickName}.local`, spellcheck: 'false', style: 'max-width:320px' });
    const line = h('p', { class: 'hint', 'aria-live': 'polite' });
    const found = h('div');
    const began = Date.now();
    const check = async () => {
      clearTimeout(timer);
      line.replaceChildren(h('span', { class: 'led' }), `Waiting for ${stickName}… (${Math.round((Date.now() - began) / 60000)} min)`);
      let s = null;
      let why = '';
      try { s = await api('POST', '/machines/stick-probe', { target: target.value }); } catch (e) { why = e.message; }
      if (!dlg.isConnected) return;
      if (s) { answered(s); return; }
      if (/let the library in/.test(why)) {
        line.replaceChildren(h('span', { class: 'error' }, `${stickName} answers but does not let the Library in: this is not the card made here (or the files were not copied onto it).`));
      } else {
        line.replaceChildren(h('span', { class: 'led' }), `Waiting for ${stickName}… (${Math.round((Date.now() - began) / 60000)} min, checking every 15 s)`);
      }
      if (Date.now() - began < 20 * 60000) timer = setTimeout(check, 15000);
      else line.replaceChildren(h('span', { class: 'error' }, `${stickName} has not answered after 20 minutes. Put the card back in the computer: wifi-report.txt on it shows what the Pi saw.`), ' ', h('button', { class: 'btn small', onclick: check }, 'Check again'));
    };
    const answered = (s) => {
      line.replaceChildren(h('span', { class: 'led on' }), `${stickName} answers.`, s.settings ? ` Stick: ${s.settings}.` : '',
        s.attached ? ' A machine is using it.' : ' Not plugged into a machine yet.');
      const free = state.machines.filter((m) => !m.stick);
      const pick = h('select', {}, free.map((m) => h('option', { value: m.id }, m.name)));
      const link = async () => {
        try { await api('PATCH', '/machines/' + pick.value, { stick: target.value }); } catch (e) { toast(e.message); return; }
        close(); await loadMachines(); toast('Stick linked'); done();
      };
      found.replaceChildren(
        h('p', {}, 'Now give it its machine:'),
        h('div', { class: 'row' },
          h('button', { class: 'btn primary', onclick: () => { close(); editMachine({ name: '', stick: target.value }, done); } }, 'New machine with this stick'),
          free.length ? [h('span', { class: 'hint' }, 'or'), pick, h('button', { class: 'btn', onclick: link }, 'Use it for this machine')] : null),
        h('p', { class: 'hint' }, 'To start work from Odoo on this machine, link the machine to its work center in Jobs › Connections.'));
    };
    dlg.replaceChildren(
      h('h2', {}, `Card files for ${stickName}`),
      h('p', {}, `The zip (${stickName}-card.zip) is in your downloads. Then:`),
      h('ol', { class: 'steps' },
        h('li', {}, 'In ', h('strong', {}, 'Raspberry Pi Imager'), ' (raspberrypi.com/software): device Raspberry Pi Zero 2 W, system ',
          h('strong', {}, 'Raspberry Pi OS (other) › Raspberry Pi OS Lite (64-bit)'), ', storage the card. When it offers to customise the settings, answer ', h('strong', {}, 'No'), '.'),
        h('li', {}, 'Take the card out and put it back in: a drive named ', h('strong', {}, 'bootfs'), ' appears.'),
        h('li', {}, 'Open the zip and copy ', h('strong', {}, 'everything in it'), ' onto bootfs, replacing the files there. Eject the card.'),
        h('li', {}, 'Put the card in the Pi and power it. The first start takes 3 to 5 minutes and restarts once.')),
      h('label', {}, 'The stick\'s address'), h('div', { class: 'row' }, target, h('button', { class: 'btn small', onclick: check }, 'Check now')),
      line, found,
      h('div', { class: 'actions' }, h('button', { class: 'btn', onclick: close }, 'Close')));
    timer = setTimeout(check, 3000);
  }
}

/** "Send to machine" for the design (and colourway) shown in the drawer. */
const threadText = (t) => t.code ? `${t.name ? t.name + ' ' : (t.chart ? t.chart + ' ' : '')}${t.code}` : (t.name || t.rgb);

/** Send the design to a machine's stick, after checking each colour's needle on that machine. */
function sendRow(d, cw) {
  const usable = state.machines.filter((m) => m.stick);
  if (!usable.length) {
    return state.machines.length
      ? h('p', { class: 'hint send-row' }, 'To send designs from here, give a machine the address of its design stick (Machines › Edit).')
      : null;
  }
  const last = lastMachine();
  const sel = h('select', { 'aria-label': 'Machine' }, usable.map((m) => h('option', { value: m.id, selected: last && m.id === last.id }, m.name)));
  const replace = h('input', { type: 'checkbox' });
  const btn = h('button', { class: 'btn small primary', disabled: true }, 'Send to machine');
  const box = h('div', { class: 'plan' }, h('p', { class: 'hint' }, 'Checking the needles…'));
  let plan = null;
  let accepted = new Set();
  const machine = () => state.machines.find((x) => x.id === Number(sel.value));
  const ready = () => plan && plan.steps.every((st, i) => st.ok || accepted.has(i));

  async function check() {
    const m = machine();
    plan = null; btn.disabled = true;
    box.replaceChildren(h('p', { class: 'hint' }, 'Checking the needles…'));
    try { plan = await api('POST', `/machines/${m.id}/needle-plan`, { design: d.id, colourway: cw ? cw.id : null }); }
    catch (e) { box.replaceChildren(h('p', { class: 'error' }, e.message)); return; }
    accepted = new Set();
    render();
  }

  async function putOn(st, n) {
    const m = machine();
    const threads = m.threads.map((t) => ({ ...t }));
    while (threads.length < n) threads.push({ rgb: '', name: '', code: '' });
    threads[n - 1] = { ...st.thread };
    try { await api('PATCH', '/machines/' + m.id, { threads }); } catch (e) { toast(e.message); return; }
    await loadMachines();
    toast(`${m.name}: needle ${n} is now ${threadText(st.thread)}. Change the cone on the machine.`);
    check();
  }

  function render() {
    const m = machine();
    const tbf = m.format === 'tbf';
    const rows = plan.steps.map((st, i) => {
      const line = h('div', { class: 'pl-line' },
        h('span', { class: 'nd-sw', style: `background:${st.thread.rgb}` }),
        h('span', {}, `${i + 1} · ${threadText(st.thread)}`),
        h('span', { class: 'pl-arrow' }, '→'),
        h('strong', {}, `needle ${st.needle}`),
        st.ok ? h('span', { class: 'pl-ok', title: 'This thread is on that needle' }, '✓') : null);
      if (st.ok) return h('li', {}, line);
      const what = st.on_needle ? `needle ${st.needle} has ${threadText(st.on_needle)}` : `needle ${st.needle} has no thread set`;
      const why = st.how === 'set' ? `Set on the design, but ${what}.` : `Not on ${m.name}: ${what}${st.how === 'closest' ? ' (the closest colour)' : ''}.`;
      const use = h('label', { class: 'check' },
        h('input', { type: 'checkbox', checked: accepted.has(i), onchange: (e) => { if (e.target.checked) accepted.add(i); else accepted.delete(i); btn.disabled = !ready(); } }),
        `Use needle ${st.needle} anyway`);
      let put = null;
      if (state.me.admin) {
        const free = Array.from({ length: m.needles }, (_, k) => k + 1).find((k) => !(m.threads[k - 1] && m.threads[k - 1].rgb));
        const pick = h('select', { 'aria-label': 'Needle' }, Array.from({ length: m.needles }, (_, k) => {
          const t = m.threads[k] && m.threads[k].rgb ? m.threads[k] : null;
          return h('option', { value: k + 1, selected: k + 1 === (free || st.needle) }, `${k + 1}${t ? ' · ' + threadText(t) : ' · empty'}`);
        }));
        put = h('span', { class: 'row' }, h('button', { class: 'btn small', onclick: () => putOn(st, Number(pick.value)) }, `Put ${threadText(st.thread)} on needle`), pick);
      }
      return h('li', { class: 'pl-warn' }, line, h('div', { class: 'pl-why' }, why), h('div', { class: 'row pl-acts' }, use, put));
    });
    const head = tbf ? 'Needles (sent with the design):' : `Needles to set on ${m.name}'s panel:`;
    box.replaceChildren(h('div', { class: 'hint' }, head), h('ol', { class: 'pl-list' }, rows));
    btn.disabled = !ready();
  }

  sel.addEventListener('change', check);
  btn.addEventListener('click', async () => {
    const m = machine();
    if (!ready()) return;
    rememberMachine(m.id);
    btn.disabled = true; btn.textContent = 'Sending…';
    try {
      const r = await api('POST', `/machines/${m.id}/stick`, { designs: [{ design: d.id, colourway: cw ? cw.id : null, needles: plan.steps.map((st) => st.needle) }], replace: replace.checked });
      toast(`Sent ${r.sent.join(', ')} to ${m.name}. ${r.stick.designs.length} design${r.stick.designs.length === 1 ? '' : 's'} on its stick.`);
    } catch (e) { toast(e.message); }
    btn.textContent = 'Send to machine'; btn.disabled = !ready();
  });
  check();
  return h('div', { class: 'send-wrap' }, h('div', { class: 'row send-row' }, btn, sel, h('label', { class: 'check' }, replace, 'Replace what is on the stick')), box);
}

/** Choose the needles from a machine's needle threads. */
function needlesFromMachine(d, cw) {
  const set = state.machines.filter((m) => m.threads.some((t) => t.rgb));
  if (!set.length) return null;
  const go = async (m) => {
    try { await api('POST', `/designs/${d.id}/needles-from-machine`, { machine: m.id, colourway: cw ? cw.id : null }); }
    catch (e) { toast(e.message); return; }
    toast(`Needles chosen from ${m.name}.`);
    openDesign(d.id);
  };
  if (set.length === 1) return h('button', { class: 'btn small', onclick: () => go(set[0]) }, `Needles from ${set[0].name}`);
  const sel = h('select', { 'aria-label': 'Machine for needles' }, set.map((m) => h('option', { value: m.id }, m.name)));
  return h('span', { class: 'row' }, h('button', { class: 'btn small', onclick: () => go(set.find((m) => m.id === Number(sel.value))) }, 'Needles from'), sel);
}

// ---------- jobs, and connections to other systems ----------

const JOB_STATUS = { waiting: 'Waiting', started: 'Started', done: 'Done', cancelled: 'Cancelled', problem: 'Problem' };
const day = (t) => (t ? new Date(t * 1000).toLocaleDateString() : '');
const clock = (t) => (t ? new Date(t * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : '');

/** Find a design by name: a search box with a list of matches; onpick({id, name}). */
function designFinder(onpick, placeholder = 'Find a design…') {
  const input = h('input', { type: 'search', placeholder, 'aria-label': 'Find a design' });
  const out = h('div', { class: 'finder-out' });
  const go = debounce(async () => {
    const q = input.value.trim();
    if (q.length < 2) { out.replaceChildren(); return; }
    let r;
    try { r = await api('GET', '/designs?' + new URLSearchParams({ q, limit: 8 })); } catch (e) { out.textContent = e.message; return; }
    out.replaceChildren(...r.items.map((d) => h('button', { class: 'chip-btn', onclick: () => { out.replaceChildren(); input.value = ''; onpick(d); } }, d.name)),
      r.items.length ? null : h('span', { class: 'hint' }, 'No design matches.'));
  }, 250);
  input.addEventListener('input', go);
  return h('div', { class: 'finder' }, input, out);
}

async function showJobs(page) {
  const main = h('main', { class: 'main jobs' });
  page.replaceChildren(main);
  await loadMachines();
  const list = h('div', {});
  const head = h('div', { class: 'row', style: 'justify-content:space-between; margin-bottom:10px' },
    h('div', {}, h('h2', { style: 'margin:0' }, 'Jobs'),
      h('div', { class: 'hint' }, 'Designs to sew. A job made by a connection (an Odoo work order) is loaded on its machine\'s stick when the work starts.')),
    h('button', { class: 'btn small', onclick: () => editJob(null, render) }, '+ New job'));
  const connBox = h('div', {});
  main.append(head, list, connBox);

  async function render() {
    if (!document.body.contains(list)) return;
    let jobs;
    try { jobs = await api('GET', '/jobs'); } catch (e) { list.replaceChildren(h('p', { class: 'error' }, e.message)); return; }
    const mname = (id) => (state.machines.find((m) => m.id === id) || {}).name || '—';
    list.replaceChildren(jobs.length ? h('table', { class: 'list jobs-table' },
      h('tr', {}, ['', 'Job', 'Design', 'Machine', 'Qty', 'Customer', 'Due', ''].map((t) => h('th', {}, t))),
      jobs.map((j) => h('tr', { class: 'job ' + j.status },
        h('td', {}, h('span', { class: 'badge ' + j.status }, JOB_STATUS[j.status] || j.status)),
        h('td', {}, h('div', {}, j.name), h('div', { class: 'hint' }, j.reference),
          j.message ? h('div', { class: j.status === 'problem' ? 'error small' : 'hint' }, (j.loaded ? clock(j.loaded) + ' · ' : '') + j.message) : null),
        h('td', {}, j.design_id
          ? h('span', {}, h('button', { class: 'link', onclick: () => openDesign(j.design_id) }, j.design_name),
            j.colourway_name ? h('span', { class: 'hint' }, ' · ' + j.colourway_name) : null, ' ',
            h('button', { class: 'link small-link', title: 'Choose another design', onclick: () => chooseJobDesign(j, render) }, 'change'))
          : h('button', { class: 'btn small choose', onclick: () => chooseJobDesign(j, render) }, 'Choose the design…'),
          !j.design_id && j.design_hint ? h('div', { class: 'hint' }, `“${j.design_hint}”`) : null),
        h('td', {}, mname(j.machine_id)),
        h('td', {}, String(j.quantity)),
        h('td', {}, j.customer_label || j.customer,
          j.customer_label && j.customer && j.customer !== j.customer_label ? h('div', { class: 'hint' }, j.customer) : null),
        h('td', {}, day(j.due)),
        h('td', { class: 'acts' },
          j.design_id && j.machine_id ? h('button', { class: 'btn small', onclick: async (e) => {
            e.target.disabled = true; e.target.textContent = 'Loading…';
            try { const r = await api('POST', `/jobs/${j.id}/load`); toast(r.message); } catch (x) { toast(x.message); }
            render();
          } }, 'Load now') : null,
          h('button', { class: 'btn small', onclick: () => editJob(j, render) }, 'Edit'))))) 
      : h('p', { class: 'hint' }, 'No jobs yet.'));
  }
  await render();
  const timer = setInterval(() => { if (document.body.contains(list)) render(); else clearInterval(timer); }, 10000);
  if (state.me.admin) showConnections(connBox, render);
}

/** Choose the design a job sews, and its colourway: designs sharing a code with its name first,
 * or any by search, each with its preview; then the chosen one large, in each colourway. */
async function chooseJobDesign(j, done) {
  let r = { suggestions: [], can_link: false };
  try { r = await api('GET', `/jobs/${j.id}/design`); } catch (e) { toast(e.message); }
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  shade.addEventListener('click', close);
  const every = h('input', { type: 'checkbox', checked: r.can_link });
  const msg = h('div', { class: 'error' });
  const found = h('div', { class: 'dtiles' });
  const chosen = h('div', { class: 'chosen' }, h('p', { class: 'hint' }, 'Pick a design to see it here.'));
  let pick = null; // { d, cw }
  const useBtn = h('button', { class: 'btn primary', disabled: true }, 'Use this design');

  const tile = (d) => h('button', { class: 'dtile' + (pick && pick.d.id === d.id ? ' on' : ''), title: d.name, onclick: () => choose(d.id) },
    h('div', { class: 'dthumb', style: fabricStyle(d.background) }, d.preview ? h('img', { src: previewUrl(d, 320), alt: '', loading: 'lazy' }) : h('span', { class: 'hint' }, 'no preview')),
    h('div', { class: 'dname' }, d.name));
  const showTiles = (list, empty) => found.replaceChildren(...(list.length ? list.map(tile) : [h('p', { class: 'hint' }, empty)]));
  const mine = r.customer_designs || [];
  // what the job's name points to, then the customer's own designs
  const showDefault = () => {
    const seen = new Set(r.suggestions.map((d) => d.id));
    const theirs = mine.filter((d) => !seen.has(d.id));
    if (!r.suggestions.length && !theirs.length) { showTiles([], 'No design shares a code with this job: search by name.'); return; }
    found.replaceChildren(
      ...(r.suggestions.length ? [h('div', { class: 'hint tiles-head' }, 'Same code in the name:'), ...r.suggestions.map(tile)] : []),
      ...(theirs.length ? [h('div', { class: 'hint tiles-head' }, `Designs of ${r.customer}:`), ...theirs.map(tile)] : []));
  };

  async function choose(id, cwId = undefined) {
    let d;
    try { d = await api('GET', '/designs/' + id); } catch (e) { msg.textContent = e.message; return; }
    const cws = d.colourways || [];
    const cw = cwId === undefined ? (j.design_id === d.id ? cws.find((c) => c.id === j.colourway_id) || null : null) : cws.find((c) => c.id === cwId) || null;
    pick = { d, cw };
    found.querySelectorAll('.dtile').forEach((el) => el.classList.toggle('on', el.title === d.name));
    const bg = cw ? cw.background : d.background;
    const threads = cw ? cw.threads : d.threads;
    chosen.replaceChildren(
      h('div', { class: 'big', style: fabricStyle(bg) }, d.preview ? h('img', { src: previewUrl(d, 1000, cw ? cw.id : null), alt: 'Preview of ' + d.name }) : h('p', { class: 'hint' }, 'No preview')),
      h('div', { class: 'row', style: 'justify-content:space-between; margin-top:6px' }, h('strong', {}, d.name),
        h('span', { class: 'hint' }, d.stitches ? `${num(d.stitches)} stitches · ${mm(d.width_mm)} × ${mm(d.height_mm)} mm` : '')),
      h('div', { class: 'swatches' }, threads.map((t) => h('span', { class: 'sw', style: `background:${t.rgb}`, title: t.code ? `${t.chart} ${t.code}` : t.rgb }))),
      h('label', {}, 'Colours'),
      h('div', { class: 'segs', role: 'group', 'aria-label': 'Colourway' },
        h('button', { class: 'seg' + (cw ? '' : ' on'), onclick: () => choose(d.id, null) },
          d.background ? h('span', { class: 'fabric-dot', style: `background:${d.background}` }) : null, 'Main colours'),
        cws.map((c) => h('button', { class: 'seg' + (cw && c.id === cw.id ? ' on' : ''), onclick: () => choose(d.id, c.id) },
          c.background ? h('span', { class: 'fabric-dot', style: `background:${c.background}` }) : null, c.name))),
      cws.length ? null : h('p', { class: 'hint' }, 'This design has no colourways: it is sewn in its main colours.'));
    useBtn.disabled = false;
    useBtn.textContent = cw ? `Use it, ${cw.name}` : 'Use it, main colours';
  }

  const save = async (d, cw) => {
    try {
      const out = await api('POST', `/jobs/${j.id}/design`, { design_id: d ? d.id : null, colourway_id: cw ? cw.id : null, every: r.can_link && every.checked });
      toast(d ? `${j.name}: ${d.name}${cw ? ' · ' + cw.name : ''}` + (out.others ? ` (and ${out.others} other job${out.others === 1 ? '' : 's'})` : '') : 'Design removed from the job.');
    } catch (e) { msg.textContent = e.message; return; }
    close(); done();
  };
  useBtn.addEventListener('click', () => pick && save(pick.d, pick.cw));

  const search = h('input', { type: 'search', placeholder: 'Find a design by name…', 'aria-label': 'Find a design' });
  const go = debounce(async () => {
    const q = search.value.trim();
    if (q.length < 2) { showDefault(); return; }
    try { const res = await api('GET', '/designs?' + new URLSearchParams({ q, limit: 24 })); showTiles(res.items, 'No design matches.'); }
    catch (e) { msg.textContent = e.message; }
  }, 250);
  search.addEventListener('input', go);

  const dlg = h('div', { class: 'dialog wide', role: 'dialog', 'aria-label': 'Choose the design' },
    h('h2', {}, 'Design for this job'),
    h('p', { style: 'margin:0' }, j.name),
    j.design_hint ? h('p', { class: 'hint', style: 'margin-top:2px' }, `In Odoo: “${j.design_hint}”`) : null,
    h('div', { class: 'pick-cols' },
      h('div', {}, search, h('div', { style: 'height:6px' }), found),
      chosen),
    r.can_link ? h('label', { class: 'check', style: 'margin-top:12px' }, every, 'Use this design and colours for every job of this operation, now and later') : null,
    msg,
    h('div', { class: 'actions', style: 'justify-content:space-between' },
      j.design_id ? h('button', { class: 'btn small', onclick: () => save(null, null) }, 'No design') : h('span'),
      h('div', { class: 'row' }, h('button', { class: 'btn', onclick: close }, 'Cancel'), useBtn)));
  document.body.append(shade, dlg);
  showDefault();
  if (j.design_id) choose(j.design_id);
  else if (r.suggestions.length === 1) choose(r.suggestions[0].id);
  search.focus();
}

function editJob(j, done) {
  const isNew = !j;
  j = j || { name: '', design_id: null, design_name: '', quantity: 1, machine_id: null, customer: '', notes: '', status: 'waiting' };
  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  shade.addEventListener('click', close);
  let design = j.design_id ? { id: j.design_id, name: j.design_name } : null;
  const dname = h('strong', {}, design ? design.name : 'none');
  const name = h('input', { type: 'text', value: j.name });
  const qty = h('input', { type: 'number', min: 1, value: j.quantity, style: 'max-width:100px' });
  const customer = h('input', { type: 'text', value: j.customer });
  const machine = h('select', {}, h('option', { value: '' }, '—'),
    state.machines.map((m) => h('option', { value: m.id, selected: m.id === j.machine_id }, m.name)));
  const status = h('select', {}, Object.entries(JOB_STATUS).map(([k, v]) => h('option', { value: k, selected: k === j.status }, v)));
  const notes = h('textarea', { rows: 3 }, j.notes);
  const msg = h('div', { class: 'error' });
  const dlg = h('div', { class: 'dialog', role: 'dialog', 'aria-label': isNew ? 'New job' : 'Job' },
    h('h2', {}, isNew ? 'New job' : j.name),
    h('label', {}, 'Name'), name,
    h('label', {}, 'Design ', dname), designFinder((d) => { design = d; dname.textContent = d.name; }),
    j.design_hint ? h('p', { class: 'hint' }, `In the other system: “${j.design_hint}”. Link it once under Connections › Designs and every job of it gets the design.`) : null,
    h('div', { class: 'row', style: 'margin-top:8px' },
      h('div', { style: 'flex:1' }, h('label', {}, 'Machine'), machine),
      h('div', {}, h('label', {}, 'Quantity'), qty),
      h('div', {}, h('label', {}, 'Status'), status)),
    h('label', {}, 'Customer'), customer,
    h('label', {}, 'Notes'), notes, msg,
    h('div', { class: 'actions', style: 'justify-content:space-between' },
      !isNew && state.me.admin ? armed('Delete', 'Delete the job?', async () => { await api('DELETE', '/jobs/' + j.id); close(); done(); }, 'btn small danger') : h('span'),
      h('div', { class: 'row' },
        h('button', { class: 'btn', onclick: close }, 'Cancel'),
        h('button', { class: 'btn primary', onclick: async () => {
          const body = { name: name.value, design_id: design ? design.id : null, quantity: Number(qty.value) || 1,
            machine_id: machine.value ? Number(machine.value) : null, customer: customer.value, notes: notes.value, status: status.value };
          try { if (isNew) await api('POST', '/jobs', body); else await api('PATCH', '/jobs/' + j.id, body); }
          catch (e) { msg.textContent = e.message; return; }
          close(); done();
        } }, 'Save'))));
  document.body.append(shade, dlg);
  name.focus();
}

/** Administrators: the connections to other systems (Odoo), their machines and designs. */
async function showConnections(box, refreshJobs) {
  let conns;
  try { conns = await api('GET', '/connections'); } catch (e) { box.replaceChildren(h('p', { class: 'error' }, e.message)); return; }
  const again = () => showConnections(box, refreshJobs);
  box.replaceChildren(h('h2', { style: 'margin-top:28px' }, 'Connections'),
    h('p', { class: 'hint' }, 'Optional. The library works without any: a connection makes jobs from another system\'s work (Odoo work orders on the work centers linked to machines) and loads the design when the work is started, e.g. in Odoo\'s Shop Floor.'),
    ...conns.map((c) => connectionCard(c, again, refreshJobs)),
    conns.length ? null : h('button', { class: 'btn small', onclick: async () => {
      try { await api('POST', '/connections', { kind: 'odoo', name: 'Odoo', enabled: false, settings: { url: '', database: '', notes: false } }); } catch (e) { toast(e.message); }
      again();
    } }, '+ Connect to Odoo'));
}

function connectionCard(c, again, refreshJobs) {
  const st = c.settings || {};
  const name = h('input', { type: 'text', value: c.name });
  const url = h('input', { type: 'text', value: st.url || '', placeholder: 'https://odoo.example.com' });
  const database = h('input', { type: 'text', value: st.database || '', placeholder: 'only if the server has several' });
  // a plain text box shown as dots: a password box makes password managers offer to save it
  // (and their pop-ups can get stuck on the page); the attributes ask the common ones to ignore it
  const key = h('input', { type: 'text', class: 'secret', autocomplete: 'off', spellcheck: 'false', name: 'ec-connection-key',
    'data-1p-ignore': 'true', 'data-lpignore': 'true', 'data-bwignore': 'true', 'data-form-type': 'other',
    placeholder: c.has_secret ? 'saved (type a new one to replace it)' : 'API key from Odoo › My Preferences › Account Security' });
  const notes = h('input', { type: 'checkbox', checked: !!st.notes });
  const enabled = h('input', { type: 'checkbox', checked: c.enabled });
  const poll = h('input', { type: 'number', min: 5, max: 3600, value: c.poll_seconds, style: 'max-width:90px' });
  const out = h('div', { class: 'hint' }, c.last_error ? h('span', { class: 'error' }, c.last_error) : (c.last_sync ? 'Last checked ' + clock(c.last_sync) : 'Not checked yet.'));
  const save = async () => {
    await api('PATCH', '/connections/' + c.id, { name: name.value, secret: key.value, enabled: enabled.checked, poll_seconds: Number(poll.value) || 10,
      settings: { url: url.value.trim(), database: database.value.trim(), notes: notes.checked } });
    key.value = '';
  };
  const run = (label, f) => h('button', { class: 'btn small', onclick: async (e) => {
    const b = e.target; const t = b.textContent; b.disabled = true; b.textContent = label + '…';
    try { await f(); } catch (x) { out.replaceChildren(h('span', { class: 'error' }, x.message)); }
    b.disabled = false; b.textContent = t;
  } }, label);
  const machinesBox = h('div', {});
  const designsBox = h('div', {});
  const card = h('section', { class: 'mcard conn' },
    h('h3', { style: 'margin-top:0' }, c.name, h('span', { class: 'hint' }, ` · ${c.kind}`)),
    h('div', { class: 'two-cols' },
      h('div', {}, h('label', {}, 'Name'), name, h('label', {}, 'Address'), url, h('label', {}, 'Database'), database),
      h('div', {}, h('label', {}, 'API key'), key,
        h('label', { class: 'check' }, enabled, 'On: check for work every ', poll, ' seconds'),
        h('label', { class: 'check' }, notes, 'Write a note on the MO when a design is loaded (or could not be)'))),
    h('div', { class: 'row', style: 'margin-top:10px; flex-wrap:wrap' },
      run('Save', async () => { await save(); out.replaceChildren(h('span', { class: 'saved' }, 'Saved.')); }),
      run('Test', async () => { await save(); const r = await api('POST', `/connections/${c.id}/test`); out.replaceChildren(h('span', { class: 'saved' }, r.message)); }),
      run('Machines', async () => { await save(); await linkMachines(c, machinesBox); }),
      run('Designs', async () => { await linkDesigns(c, designsBox); }),
      run('Customers', async () => { await save(); await linkCustomers(c); }),
      run('Check now', async () => {
        await save();
        const r = await api('POST', `/connections/${c.id}/sync`);
        out.replaceChildren(h('span', { class: 'saved' }, `${r.seen} work orders, ${r.new_jobs} new jobs` + (r.loaded.length ? `; loaded: ${r.loaded.join('; ')}` : '')),
          ...r.problems.map((p) => h('div', { class: 'error' }, p)));
        refreshJobs();
      }),
      armed('Remove', 'Remove the connection? (its jobs stay)', async () => { await api('DELETE', '/connections/' + c.id); again(); }, 'btn small danger')),
    out, machinesBox, designsBox);
  return card;
}

/** The library's customers beside the other system's: link the sure ones at once, check the
 * likely ones, pick the rest from the list. */
async function linkCustomers(c) {
  let r;
  try { r = await api('GET', `/connections/${c.id}/customers`); } catch (e) { toast(e.message); return; }
  const remoteText = (x) => `${x.name}${x.old_number ? ' [' + x.old_number + ']' : ''}${x.reference ? ' · ' + x.reference : ''}`;
  const byText = new Map(r.remote.map((x) => [remoteText(x), x]));
  const list = h('datalist', { id: 'ec-remote-customers' }, r.remote.map((x) => h('option', { value: remoteText(x) })));
  const custText = (m) => m.customer.old_number && m.customer.old_number.toLowerCase() !== m.customer.name.toLowerCase()
    ? `${m.customer.name} (${m.customer.old_number})` : m.customer.name;
  const kind = (m) => (m.linked ? 'linked' : m.sure ? 'sure' : m.maybe.length ? 'maybe' : 'none');
  const takeName = h('input', { type: 'checkbox' });
  const filter = h('input', { type: 'search', placeholder: 'Find a customer…', 'aria-label': 'Find a customer' });
  const rows = h('div', { class: 'cust-rows' });
  const tabsBox = h('div', { class: 'segs', role: 'tablist' });
  const msg = h('div', { class: 'error', role: 'alert' });
  let tab = r.matches.some((m) => kind(m) === 'sure') ? 'sure' : 'maybe';

  const send = async (links) => {
    msg.textContent = '';
    try { await api('PUT', `/connections/${c.id}/customers`, { links }); } catch (e) { msg.textContent = e.message; return false; }
    return true;
  };
  // after linking, the library may have taken the name or old number: shown as the server did it
  const linked = (m, x) => {
    m.linked = x; m.sure = null; m.maybe = [];
    if (!m.customer.old_number && /^[A-Za-z]{3}\d[A-Za-z]\d$/.test(x.old_number || '')) m.customer.old_number = x.old_number.toUpperCase();
    if (takeName.checked || m.customer.name.toLowerCase() === (m.customer.old_number || '').toLowerCase()) m.customer.name = x.name;
  };
  const linkOne = async (m, x) => {
    if (!x) { toast('Choose an Odoo customer from the list.'); return; }
    if (await send([{ customer_id: m.customer.id, remote: x, take_name: takeName.checked }])) { linked(m, x); draw(); }
  };

  const row = (m) => {
    const k = kind(m);
    const who = h('div', { class: 'cust-who' }, h('strong', {}, custText(m)),
      h('div', { class: 'hint' }, `${num(m.customer.designs)} design${m.customer.designs === 1 ? '' : 's'}${m.customer.children ? ` · ${m.customer.children} under it` : ''}`));
    let side;
    if (k === 'linked') {
      side = h('div', { class: 'row' }, h('span', { class: 'saved' }, '✓ ', remoteText(m.linked)),
        h('button', { class: 'link', onclick: async () => {
          if (await send([{ customer_id: m.customer.id, remote: null }])) { m.linked = null; draw(); }
        } }, 'Unlink'));
    } else if (k === 'sure') {
      side = h('div', { class: 'row' }, h('span', {}, remoteText(m.sure)), h('span', { class: 'hint' }, `same ${m.how}`),
        h('button', { class: 'btn small primary', onclick: () => linkOne(m, m.sure) }, 'Link'));
    } else {
      const pickIn = h('input', { type: 'text', list: 'ec-remote-customers', placeholder: 'Search the Odoo customers…', style: 'min-width:260px' });
      side = h('div', {},
        m.maybe.length ? h('div', { class: 'row', style: 'flex-wrap:wrap; margin-bottom:4px' }, m.maybe.map((x) =>
          h('button', { class: 'btn small', title: 'Link to this one', onclick: () => linkOne(m, x) }, remoteText(x)))) : null,
        h('div', { class: 'row' }, pickIn, h('button', { class: 'btn small', onclick: () => linkOne(m, byText.get(pickIn.value)) }, 'Link')));
    }
    return h('div', { class: 'cust-row' }, who, side);
  };

  function draw() {
    const counts = { sure: 0, maybe: 0, none: 0, linked: 0 };
    r.matches.forEach((m) => { counts[kind(m)] += 1; });
    const labels = { sure: 'Sure matches', maybe: 'To check', none: 'Not found', linked: 'Linked' };
    tabsBox.replaceChildren(...Object.keys(labels).map((t) => h('button', { class: 'seg' + (t === tab ? ' on' : ''), role: 'tab', 'aria-selected': t === tab ? 'true' : 'false',
      onclick: () => { tab = t; draw(); } }, `${labels[t]} (${counts[t]})`)));
    const q = filter.value.trim().toLowerCase();
    const shown = r.matches.filter((m) => kind(m) === tab && (!q || custText(m).toLowerCase().includes(q)));
    const sure = r.matches.filter((m) => kind(m) === 'sure');
    rows.replaceChildren(
      tab === 'sure' && sure.length ? h('div', { class: 'row', style: 'margin-bottom:8px' },
        h('button', { class: 'btn primary', onclick: async (e) => {
          e.target.disabled = true;
          if (await send(sure.map((m) => ({ customer_id: m.customer.id, remote: m.sure, take_name: takeName.checked })))) {
            sure.forEach((m) => linked(m, m.sure));
            toast(`${sure.length} customers linked.`);
            tab = 'maybe';
          }
          draw();
        } }, `Link all ${sure.length} sure matches`),
        h('span', { class: 'hint' }, 'Same old number (Acomba) or same name in both.')) : null,
      tab === 'none' ? h('p', { class: 'hint' }, 'No Odoo customer with this old number or a name like it. Search the Odoo list, or leave it: work from Odoo for a customer not linked adds or finds it by itself.') : null,
      ...(shown.length ? shown.slice(0, 300).map(row) : [h('p', { class: 'hint' }, 'Nothing here.')]),
      shown.length > 300 ? h('p', { class: 'hint' }, `${shown.length - 300} more: find them by name.`) : null);
  }
  filter.addEventListener('input', debounce(draw, 150));

  const shade = h('div', { class: 'dialog-shade' });
  const close = () => { shade.remove(); dlg.remove(); };
  shade.addEventListener('click', close);
  const dlg = h('div', { class: 'dialog wide', role: 'dialog', 'aria-label': 'Customers' },
    h('h2', {}, `Customers in ${c.name}`),
    h('p', { class: 'hint' }, `The library's ${r.matches.length} customers beside Odoo's ${r.remote.length}. A linked customer's jobs show its designs first, and its name and old number (Acomba) are filled in from Odoo when the library only had the number.`),
    h('div', { class: 'row', style: 'justify-content:space-between; flex-wrap:wrap; gap:8px' }, tabsBox, filter),
    h('label', { class: 'check', style: 'margin:8px 0' }, takeName, 'Also rename the customers I link to their Odoo name'),
    msg, rows, list,
    h('div', { class: 'actions' }, h('button', { class: 'btn', onclick: () => { close(); loadCustomers().catch(() => {}); } }, 'Close')));
  document.body.append(shade, dlg);
  draw();
}

async function linkMachines(c, box) {
  const r = await api('GET', `/connections/${c.id}/machines`);
  const sels = r.machines.map((row) => ({ row, sel: h('select', {}, h('option', { value: '' }, '— not a machine —'),
    state.machines.map((m) => h('option', { value: m.id, selected: m.id === row.machine_id }, m.name))) }));
  box.replaceChildren(h('h4', {}, 'Machines'),
    h('p', { class: 'hint' }, 'Which work center is which machine. Work orders on the linked work centers become jobs.'),
    h('table', { class: 'list' }, sels.map(({ row, sel }) => h('tr', {}, h('td', {}, row.remote.label), h('td', {}, sel)))),
    h('button', { class: 'btn small primary', onclick: async () => {
      try {
        await api('PUT', `/connections/${c.id}/machines`, { links: sels.map(({ row, sel }) => ({ model: row.remote.model, external_id: row.remote.id, label: row.remote.label, machine_id: sel.value ? Number(sel.value) : null })) });
        toast('Machines linked.');
      } catch (e) { toast(e.message); }
    } }, 'Save machines'));
}

async function linkDesigns(c, box) {
  box.replaceChildren(h('p', { class: 'hint' }, 'Asking Odoo…'));
  const r = await api('GET', `/connections/${c.id}/designs`);
  const set = async (row, d) => {
    try { await api('PUT', `/connections/${c.id}/designs`, { model: row.remote.model, external_id: row.remote.id, label: row.remote.label, design_id: d ? d.id : null }); }
    catch (e) { toast(e.message); return; }
    linkDesigns(c, box);
  };
  box.replaceChildren(h('h4', {}, 'Designs'),
    h('p', { class: 'hint' }, r.designs.length ? 'Which design each operation sews. Linked once, every work order of that operation gets the design.' : 'No operations: link machines first, and put operations on their work centers.'),
    h('table', { class: 'list' }, r.designs.map((row) => h('tr', {},
      h('td', {}, row.remote.label),
      h('td', {}, row.design
        ? h('span', {}, h('button', { class: 'link', onclick: () => openDesign(row.design.id) }, row.design.name), ' ', h('button', { class: 'link', onclick: () => set(row, null) }, 'unlink'))
        : h('div', {},
          row.suggestions.length ? h('div', { class: 'row', style: 'flex-wrap:wrap; gap:4px' }, h('span', { class: 'hint' }, 'Same code:'),
            row.suggestions.map((d) => h('button', { class: 'chip-btn', onclick: () => set(row, d) }, d.name))) : null,
          designFinder((d) => set(row, d))))))));
}
