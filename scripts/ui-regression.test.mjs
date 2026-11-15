import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { dirname, resolve, extname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

// This suite executes the real TypeScript module and its relative imports. The
// DOM/IPC below are deliberately small, deterministic doubles, not a browser,
// WebView2 integration test, or a physical monitor/registry test.
const mainPath = fileURLToPath(new URL('../ui/main.ts', import.meta.url));
const decode = text => text.replace(/&quot;/g, '"').replace(/&#39;/g, "'")
  .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');

class ElementDouble {
  constructor(tagName = 'div', owner = null) {
    this.tagName = tagName.toUpperCase();
    this.ownerDocument = owner;
    this.attributes = new Map();
    this.children = [];
    this.listeners = new Map();
    this.dataset = {};
    this.checked = false;
    this.disabled = false;
    this.hidden = false;
    this.inert = false;
    this.open = false;
    this._value = '';
    this._text = '';
    this._html = '';
    this.validationMessage = '';
    this.style = { setProperty() {}, removeProperty() {} };
    this.classList = { toggle() {}, add() {}, remove() {}, contains() { return false; } };
  }
  setAttribute(name, value) {
    this.attributes.set(name, String(value));
    if (name === 'id') { this.id = String(value); this.ownerDocument?.nodes.set(this.id, this); }
    if (['type', 'min', 'max', 'step', 'name'].includes(name)) this[name] = String(value);
    if (name === 'value') this._value = String(value);
    if (name === 'disabled') this.disabled = true;
    if (name === 'checked') this.checked = true;
    if (name.startsWith('data-')) this.dataset[name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = String(value);
  }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  removeAttribute(name) { this.attributes.delete(name); }
  toggleAttribute(name, force) {
    const enabled = force ?? !this.attributes.has(name);
    if (enabled) this.setAttribute(name, ''); else this.removeAttribute(name);
    return enabled;
  }
  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener); this.listeners.set(type, listeners);
  }
  dispatch(type) {
    return Promise.all((this.listeners.get(type) ?? []).map(listener => listener({ target: this, preventDefault() {} })));
  }
  append(...children) { for (const child of children) { child.parentElement = this; this.children.push(child); } }
  appendChild(child) { this.append(child); return child; }
  replaceChildren(...children) {
    this.children = []; this.append(...children.flat());
    if (this.tagName === 'SELECT' && !this.options.some(option => option.value === this._value)) this._value = this.options[0]?.value ?? '';
  }
  remove() { if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(node => node !== this); }
  focus() {}
  setCustomValidity(message) { this.validationMessage = String(message); }
  reportValidity() { return this.validationMessage === ''; }
  showModal() { this.open = true; }
  close(returnValue) { if (returnValue !== undefined) this.returnValue = returnValue; this.open = false; return this.dispatch('close'); }
  get options() { return this.children.filter(child => child.tagName === 'OPTION'); }
  get value() { return this._value; }
  set value(value) {
    const text = String(value);
    this._value = this.tagName === 'SELECT' && !this.options.some(option => option.value === text) ? '' : text;
  }
  get valueAsNumber() { return this.value === '' ? NaN : Number(this.value); }
  set valueAsNumber(value) { this.value = String(value); }
  get textContent() { return this._text; }
  set textContent(value) { this._text = String(value); this._html = ''; }
  get innerHTML() {
    return this._html || this._text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }
  set innerHTML(html) {
    this._html = String(html); this._text = '';
    if (this.tagName === 'SELECT') {
      this.replaceChildren(...parseOptions(this._html, this.ownerDocument));
    } else if (this.id === 'app') {
      this.ownerDocument.parse(this._html);
    }
  }
}

function parseAttributes(source, element) {
  for (const match of source.matchAll(/([\w-]+)(?:="([^"]*)"|='([^']*)'|=([^\s>]+))?/g)) {
    element.setAttribute(match[1], decode(match[2] ?? match[3] ?? match[4] ?? ''));
  }
}

function parseOptions(html, document) {
  return [...html.matchAll(/<option\b([^>]*)>([\s\S]*?)<\/option>/g)].map(match => {
    const option = new ElementDouble('option', document);
    parseAttributes(match[1], option); option.textContent = decode(match[2]);
    return option;
  });
}

function createDocument() {
  const document = {
    nodes: new Map(), hidden: false,
    createElement(tagName) { return new ElementDouble(tagName, this); },
    getElementById(id) { return this.nodes.get(id) ?? null; },
    addEventListener() {},
    querySelector(selector) {
      if (selector.startsWith('#')) return this.getElementById(selector.slice(1));
      let row = this.nodes.get(selector);
      if (!row) { row = this.createElement('div'); this.nodes.set(selector, row); }
      return row;
    },
    querySelectorAll(selector) {
      if (selector === 'input[type="number"]') return [...this.nodes.values()].filter(node => node.tagName === 'INPUT' && node.type === 'number');
      return [];
    },
    parse(html) {
      for (const match of html.matchAll(/<([a-z][\w-]*)\b([^>]*\bid="[^"]+"[^>]*)>/gi)) {
        const node = this.createElement(match[1]); parseAttributes(match[2], node);
      }
      for (const match of html.matchAll(/<select\b([^>]*)>([\s\S]*?)<\/select>/gi)) {
        const id = match[1].match(/\bid="([^"]+)"/)?.[1];
        if (id) this.getElementById(id).innerHTML = match[2];
      }
    },
  };
  document.documentElement = document.createElement('html');
  document.body = document.createElement('body');
  const app = document.createElement('div'); app.setAttribute('id', 'app');
  return document;
}

function stateFixture() {
  return {
    settings: {
      targetStableId: 'screen-1', mode: 'Daily', themeMode: 'Dark', showActiveWindow: false,
      customProfile: {
        leaveDelayMilliseconds: 1500, idleBlackoutMilliseconds: 30000,
        blackoutRecoveryMilliseconds: 500, brightnessRecoveryMilliseconds: 500,
        brightnessRecoveryFrames: 5, brightnessFadeDurationMilliseconds: 300,
        brightnessFadeEnabled: true, blackoutFadeEnabled: true,
        brightnessFadeFrames: 30, brightnessFadeExponent: 2.4,
      },
    },
    monitors: [{ stableId: 'screen-1', name: 'First monitor', gdiName: 'DISPLAY1', bounds: { left: 0, top: 0, right: 1440, bottom: 2560 }, primary: false }],
    startupEnabled: false, version: '1.2.1', shell: 'Tauri', status: 'Ready',
    brightnessDimmed: false, overlayOpacity: 0,
  };
}

const integerProfileKeys = [
  'leaveDelayMilliseconds', 'idleBlackoutMilliseconds', 'blackoutRecoveryMilliseconds',
  'brightnessRecoveryMilliseconds', 'brightnessRecoveryFrames',
  'brightnessFadeDurationMilliseconds', 'brightnessFadeFrames',
];

function assertIpcContract(args) {
  for (const key of integerProfileKeys) {
    assert(Number.isInteger(args.settings.customProfile[key]), `${key} must serialize as an integer`);
    assert(args.settings.customProfile[key] >= 0 && args.settings.customProfile[key] <= 0xffffffff, `${key} must fit u32`);
  }
  assert(Number.isFinite(args.settings.customProfile.brightnessFadeExponent));
  assert(args.startupEnabled === undefined || args.startupEnabled === null || typeof args.startupEnabled === 'boolean');
}

function deferred() {
  let resolvePromise, rejectPromise;
  const promise = new Promise((resolve, reject) => { resolvePromise = resolve; rejectPromise = reject; });
  return { promise, resolve: resolvePromise, reject: rejectPromise };
}

async function createHarness() {
  const document = createDocument();
  let backend = stateFixture();
  const calls = [], pendingStates = [], timers = new Map();
  let nextTimerId = 0;
  const invoke = async (command, args) => {
    calls.push({ command, args: args === undefined ? undefined : structuredClone(args) });
    if (command === 'get_state') return pendingStates.length ? await pendingStates.shift() : structuredClone(backend);
    if (command === 'apply_settings') {
      assertIpcContract(args);
      backend.settings = structuredClone(args.settings);
      if (typeof args.startupEnabled === 'boolean') backend.startupEnabled = args.startupEnabled;
      return structuredClone(backend);
    }
    if (command === 'close_settings' || command === 'restore_brightness') return;
    if (command === 'confirm_recovery_target') {
      assert.equal(typeof args.target, 'string');
      backend.recoveryRequiresConfirmation = null;
      return structuredClone(backend);
    }
    throw new Error(`Unexpected IPC command: ${command}`);
  };
  const sandbox = {
    document, console, structuredClone, URL, HTMLElement: ElementDouble,
    HTMLInputElement: ElementDouble, HTMLSelectElement: ElementDouble, HTMLDialogElement: ElementDouble,
    Option: class extends ElementDouble {
      constructor(text, value = text) { super('option', document); this.textContent = text; this.value = value; }
    },
    matchMedia: () => ({ matches: true, addEventListener() {} }),
    setTimeout(callback, delay) { const id = ++nextTimerId; timers.set(id, { callback, delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
    setInterval(callback, delay) { const id = ++nextTimerId; timers.set(id, { callback, delay }); return id; },
    clearInterval(id) { timers.delete(id); },
  };
  sandbox.window = sandbox;
  const context = vm.createContext(sandbox);
  const cache = new Map();
  function loadModule(filename) {
    if (cache.has(filename)) return cache.get(filename).exports;
    let source = readFileSync(filename, 'utf8');
    const syntax = ts.createSourceFile(filename, source, ts.ScriptTarget.Latest, true);
    const replacements = [];
    const visit = node => {
      if (ts.isMetaProperty(node) && node.keywordToken === ts.SyntaxKind.ImportKeyword) {
        replacements.push([node.getStart(syntax), node.end, `({ url: ${JSON.stringify(pathToFileURL(filename).href)} })`]);
      }
      ts.forEachChild(node, visit);
    };
    visit(syntax);
    for (const [start, end, text] of replacements.sort((a, b) => b[0] - a[0])) source = source.slice(0, start) + text + source.slice(end);
    if (filename === mainPath) {
      assert(/\bvoid init\(\);?\s*$/.test(source), 'Update the harness if the production bootstrap changes');
      source = source.replace(/\bvoid init\(\);?\s*$/, '');
      source += `\n;globalThis.__uiHooks = {
        read: () => structuredClone({ state, draft, profile, startup, dirty, busy }),
        renderState, pollState, apply,
        mutateProfile: values => Object.assign(profile, values),
      }; globalThis.__ready = init();`;
    }
    const result = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } });
    const module = { exports: {} }; cache.set(filename, module);
    const localRequire = specifier => {
      if (specifier === '@tauri-apps/api/core') return { invoke, isTauri: () => true };
      if (specifier.endsWith('.css')) return {};
      if (specifier.startsWith('.')) {
        const candidate = resolve(dirname(filename), specifier);
        const path = extname(candidate) ? candidate : `${candidate}.ts`;
        assert(existsSync(path), `Missing relative UI module: ${specifier}`);
        return loadModule(path);
      }
      throw new Error(`Unexpected production import: ${specifier}`);
    };
    const wrapper = new vm.Script(`(function(exports, require, module, __filename, __dirname) {\n${result.outputText}\n})`, { filename });
    wrapper.runInContext(context)(module.exports, localRequire, module, filename, dirname(filename));
    return module.exports;
  }
  loadModule(mainPath);
  await sandbox.__ready;
  assert(sandbox.__uiHooks.read().draft, 'Production init completed');
  return {
    hooks: sandbox.__uiHooks, calls, timers,
    node: id => { const node = document.getElementById(id); assert(node, `Missing DOM double: ${id}`); return node; },
    setBackend(value) { backend = structuredClone(value); },
    getBackend() { return structuredClone(backend); },
    deferNextState() { const pending = deferred(); pendingStates.push(pending.promise); return pending; },
    change(id, value) { const node = document.getElementById(id); assert(node, `Missing input: ${id}`); if (typeof value === 'boolean') node.checked = value; else node.value = value; node.dispatch('change'); },
    async poll() { await sandbox.__uiHooks.pollState(); },
    async apply() { await sandbox.__uiHooks.apply(false); },
  };
}

test('F03: monitor hotplug/disconnect refresh inventory and detail without discarding an edited draft', async () => {
  const ui = await createHarness();
  ui.change('theme', 'Light');
  ui.change('leaveDelayMilliseconds-number', '2.2');
  const next = ui.getBackend();
  next.monitors[0].bounds.right = 2560;
  next.monitors.push({ ...next.monitors[0], stableId: 'screen-2', name: 'Second monitor' });
  ui.setBackend(next); await ui.poll();
  assert.deepEqual(ui.node('target').options.map(option => option.value), ['screen-1', 'screen-2']);
  assert.match(ui.node('monitor-detail').textContent, /^2560 × 2560/);
  assert.equal(ui.hooks.read().draft.themeMode, 'Light');
  assert.equal(ui.hooks.read().profile.leaveDelayMilliseconds, 2200);
  assert.equal(ui.hooks.read().dirty, true);
  next.monitors = []; ui.setBackend(next); await ui.poll();
  assert.match(ui.node('monitor-detail').textContent, /未连接/);
  assert.equal(ui.node('target').value, 'screen-1');
  assert.equal(ui.node('target').options.length, 1);
  assert.equal(ui.hooks.read().draft.themeMode, 'Light');
  assert.equal(ui.hooks.read().profile.leaveDelayMilliseconds, 2200);
});

test('F08: connected and missing monitor identities stay opaque across quotes and literal markup', async () => {
  const identities = ['screen-1" data-f08="sentinel', 'literal-&quot;-&#34;-&amp;', String.raw`\\?\DISPLAY#DELA0E7#INSTANCE#{GUID}`, `screen'><option data-f08='extra'>`];
  for (const identity of identities) {
    const ui = await createHarness();
    const next = ui.getBackend();
    const label = 'Panel <b data-f08="name">literal</b> &quot; 副屏';
    next.settings.targetStableId = identity;
    next.monitors[0].stableId = identity; next.monitors[0].name = label;
    ui.setBackend(next); await ui.poll();
    assert.equal(ui.node('target').value, identity);
    assert.equal(ui.node('target').options.length, 1);
    assert.equal(ui.node('target').options[0].value, identity);
    assert.equal(ui.node('target').options[0].textContent, label);
    assert.equal(ui.node('target').options[0].getAttribute('data-f08'), null);
    next.monitors = []; ui.setBackend(next); await ui.poll();
    assert.equal(ui.node('target').options.length, 1);
    assert.equal(ui.node('target').options[0].value, identity);
    assert.equal(ui.node('target').value, identity);
    assert.equal(ui.node('target').options[0].getAttribute('data-f08'), null);
    assert.match(ui.node('target').options[0].textContent, /未连接/);
  }
});

test('F04: an external startup toggle is synchronized and an unrelated save sends a null preference', async () => {
  const ui = await createHarness();
  const next = ui.getBackend(); next.startupEnabled = true;
  ui.setBackend(next); await ui.poll();
  assert.equal(ui.node('startup').checked, true);
  ui.change('theme', 'Light'); await ui.apply();
  const call = ui.calls.findLast(call => call.command === 'apply_settings');
  assert(call);
  assert.equal(call.args.startupEnabled, null);
  assert.equal(ui.getBackend().startupEnabled, true);
  assert.equal(ui.getBackend().settings.themeMode, 'Light');
});

test('F04: an explicitly edited startup field survives polling and is submitted once', async () => {
  const ui = await createHarness();
  ui.change('startup', true);
  await ui.poll();
  assert.equal(ui.node('startup').checked, true);
  await ui.apply();
  assert.equal(ui.calls.findLast(call => call.command === 'apply_settings').args.startupEnabled, true);
  ui.change('theme', 'Light'); await ui.apply();
  assert.equal(ui.calls.findLast(call => call.command === 'apply_settings').args.startupEnabled, null);
  assert.equal(ui.getBackend().startupEnabled, true);
});

test('F03/F04: a slow poll cannot overlap another poll or roll back a newer apply response', async () => {
  const ui = await createHarness();
  const oldState = ui.getBackend();
  const pending = ui.deferNextState();
  const firstPoll = ui.hooks.pollState();
  const duplicatePoll = ui.hooks.pollState();
  await Promise.resolve();
  assert.equal(ui.calls.filter(call => call.command === 'get_state').length, 2, 'one init request plus one in-flight poll');
  ui.change('theme', 'Light'); await ui.apply();
  assert.equal(ui.hooks.read().state.settings.themeMode, 'Light');
  pending.resolve(oldState);
  await Promise.all([firstPoll, duplicatePoll]);
  assert.equal(ui.hooks.read().state.settings.themeMode, 'Light');
  assert.equal(ui.hooks.read().draft.themeMode, 'Light');
  assert.equal(ui.node('theme').value, 'Light');
  assert.equal(ui.hooks.read().dirty, false);
});

test('F03: untouched profile controls follow external changes while theme or target is dirty', async () => {
  for (const editedField of ['theme', 'target']) {
    const ui = await createHarness();
    const initial = ui.getBackend();
    initial.monitors.push({ ...initial.monitors[0], stableId: 'screen-2', name: 'Second monitor' });
    ui.setBackend(initial); await ui.poll();
    ui.change(editedField, editedField === 'theme' ? 'Light' : 'screen-2');
    const external = ui.getBackend();
    external.settings.mode = 'Custom';
    external.settings.customProfile.brightnessRecoveryFrames = 20;
    external.settings.customProfile.brightnessFadeDurationMilliseconds = 1400;
    ui.setBackend(external); await ui.poll();
    assert.equal(ui.hooks.read().profile.brightnessRecoveryFrames, 20);
    assert.equal(ui.node('brightnessRecoveryFrames-number').value, '20');
    assert.equal(ui.node('brightnessFadeDurationMilliseconds-number').value, '1.4');
    await ui.apply();
    const saved = ui.getBackend().settings;
    assert.equal(saved.customProfile.brightnessRecoveryFrames, 20, `${editedField}: saving must not read stale controls`);
    assert.equal(saved.customProfile.brightnessFadeDurationMilliseconds, 1400);
    if (editedField === 'theme') assert.equal(saved.themeMode, 'Light');
    else assert.equal(saved.targetStableId, 'screen-2');
  }
});

test('F05: numeric edits are step-quantized before the real IPC boundary', async () => {
  const cases = [
    ['brightnessFadeFrames', '5.5', 6],
    ['brightnessFadeFrames', '5.4', 5],
    ['brightnessFadeDurationMilliseconds', '0.3333', 300],
    ['brightnessRecoveryMilliseconds', '99', 1000],
    ['brightnessRecoveryFrames', '2.75', 3],
    ['brightnessFadeFrames', '-20', 5],
    ['idleBlackoutMilliseconds', '30.6', 31000],
    ['brightnessFadeExponent', '2.45', 2.5],
  ];
  for (const [key, input, expected] of cases) {
    const ui = await createHarness();
    ui.change(`${key}-number`, input);
    await ui.apply();
    const call = ui.calls.findLast(call => call.command === 'apply_settings');
    assert(call, `${key}=${input}: valid normalized edit should reach IPC`);
    assert.equal(call.args.settings.customProfile[key], expected, `${key}=${input}`);
    assertIpcContract(call.args);
  }
});

test('F05: empty and non-finite number edits never leak invalid payloads', async () => {
  for (const input of ['', 'Infinity', 'NaN', '1e999', 'not-a-number']) {
    const ui = await createHarness();
    ui.change('brightnessFadeFrames-number', input);
    await ui.apply();
    for (const call of ui.calls.filter(call => call.command === 'apply_settings')) assertIpcContract(call.args);
    assert(Number.isInteger(ui.hooks.read().profile.brightnessFadeFrames), `input ${JSON.stringify(input)} must leave an integer draft`);
  }
});

test('F05: pending empty text survives unrelated controls and polling until corrected', async () => {
  const ui = await createHarness();
  const number = ui.node('brightnessFadeFrames-number');
  number.value = ''; await number.dispatch('input'); await number.dispatch('change');
  ui.change('blackoutFadeEnabled', false);
  await ui.poll();
  assert.equal(number.value, '', 'an updateControls call must not hide an incomplete edit');
  assert.equal(ui.hooks.read().profile.brightnessFadeFrames, 30);
  await ui.apply();
  assert.equal(ui.calls.filter(call => call.command === 'apply_settings').length, 0);
  assert.equal(ui.node('error').hidden, false);
  number.value = '45'; await number.dispatch('input');
  await ui.poll();
  assert.equal(number.value, '45', 'polling must also preserve uncommitted valid text');
  await ui.apply();
  assert.equal(ui.getBackend().settings.customProfile.brightnessFadeFrames, 45);
});

test('F05: selecting a preset explicitly clears pending numeric text and validation errors', async () => {
  const ui = await createHarness();
  ui.change('brightnessFadeFrames-number', '');
  ui.change('mode', 'Movie');
  assert.equal(ui.node('brightnessFadeFrames-number').value, '30');
  assert.equal(ui.node('brightnessFadeFrames-number').validationMessage, '');
  await ui.apply();
  assert.equal(ui.getBackend().settings.mode, 'Movie');
});

test('F05: submit-time validation rejects an invalid disabled integer field before IPC', async () => {
  const ui = await createHarness();
  ui.change('brightnessFadeEnabled', false);
  ui.hooks.mutateProfile({ brightnessFadeFrames: 5.5 });
  await ui.apply();
  assert.equal(ui.calls.filter(call => call.command === 'apply_settings').length, 0);
  assert.equal(ui.node('error').hidden, false);
  assert.equal(ui.hooks.read().busy, false);
});

test('F02/F04: recovery confirmation never applies unrelated settings or a startup draft', async () => {
  const ui = await createHarness();
  const next = ui.getBackend(); next.recoveryRequiresConfirmation = 'LEGACY-MODEL';
  ui.setBackend(next); await ui.poll();
  ui.change('theme', 'Light');
  ui.change('leaveDelayMilliseconds-number', '2.2');
  ui.change('startup', true);
  await ui.node('confirm-recovery').dispatch('click');
  assert.equal(ui.node('recovery-dialog').open, true);
  assert.equal(ui.calls.filter(call => call.command === 'confirm_recovery_target').length, 0, 'opening is not confirmation');
  await ui.node('recovery-dialog').close('confirm');
  const confirmation = ui.calls.findLast(call => call.command === 'confirm_recovery_target');
  assert(confirmation);
  assert.deepEqual(confirmation.args, { target: 'screen-1' });
  assert.equal(ui.calls.filter(call => call.command === 'apply_settings').length, 0);
  assert.equal(ui.getBackend().settings.themeMode, 'Dark');
  assert.equal(ui.getBackend().startupEnabled, false);
  assert.equal(ui.getBackend().settings.customProfile.leaveDelayMilliseconds, 1500);
  assert.equal(ui.hooks.read().draft.themeMode, 'Light');
  assert.equal(ui.hooks.read().profile.leaveDelayMilliseconds, 2200);
  assert.equal(ui.hooks.read().startup, true);
  assert.equal(ui.hooks.read().dirty, true);
});

test('F02: confirmation is rejected after the selected monitor disconnects', async () => {
  const ui = await createHarness();
  const next = ui.getBackend(); next.recoveryRequiresConfirmation = 'LEGACY-MODEL';
  ui.setBackend(next); await ui.poll();
  await ui.node('confirm-recovery').dispatch('click');
  next.monitors = []; ui.setBackend(next); await ui.poll();
  await ui.node('recovery-dialog').close('confirm');
  assert.equal(ui.calls.filter(call => call.command === 'confirm_recovery_target').length, 0);
  assert.equal(ui.node('error').hidden, false);
});

test('F02: a changed recovery record requires a fresh confirmation', async () => {
  const ui = await createHarness();
  const next = ui.getBackend(); next.recoveryRequiresConfirmation = 'FIRST-MODEL';
  ui.setBackend(next); await ui.poll();
  await ui.node('confirm-recovery').dispatch('click');
  next.recoveryRequiresConfirmation = 'SECOND-MODEL';
  ui.setBackend(next); await ui.poll();
  await ui.node('recovery-dialog').close('confirm');
  assert.equal(ui.calls.filter(call => call.command === 'confirm_recovery_target').length, 0);
  assert.equal(ui.node('error').hidden, false);
});

test('F02: reopening and cancelling a recovery dialog never reuses an earlier confirmation', async () => {
  const ui = await createHarness();
  const first = ui.getBackend(); first.recoveryRequiresConfirmation = 'FIRST-MODEL';
  ui.setBackend(first); await ui.poll();
  await ui.node('confirm-recovery').dispatch('click');
  await ui.node('recovery-dialog').close('confirm');
  assert.equal(ui.calls.filter(call => call.command === 'confirm_recovery_target').length, 1);
  const second = ui.getBackend(); second.recoveryRequiresConfirmation = 'SECOND-MODEL';
  ui.setBackend(second); await ui.poll();
  await ui.node('confirm-recovery').dispatch('click');
  assert.equal(ui.node('recovery-dialog').returnValue, 'cancel');
  await ui.node('recovery-dialog').close();
  assert.equal(ui.calls.filter(call => call.command === 'confirm_recovery_target').length, 1);
});

test('F03: a rejected poll releases its in-flight guard and preserves a pending draft', async () => {
  const ui = await createHarness();
  ui.change('theme', 'Light');
  const pending = ui.deferNextState();
  const poll = ui.hooks.pollState();
  pending.reject(new Error('Isolated transient IPC failure'));
  await poll;
  assert.equal(ui.hooks.read().draft.themeMode, 'Light');
  const next = ui.getBackend(); next.status = 'Updated after retry'; ui.setBackend(next);
  await ui.poll();
  assert.equal(ui.node('live-status').textContent, 'Updated after retry');
  assert.equal(ui.hooks.read().draft.themeMode, 'Light');
});
