import { invoke, isTauri } from '@tauri-apps/api/core';
import type { AppState, Mode, Profile, Settings, ThemeMode } from './types';
import { mergeSettingsDraft, profileValidationError, quantizeNumericInput, sliders, type NumericProfileKey, type SliderSpec } from './form-state';
import './style.css';

const daily: Profile = {
  leaveDelayMilliseconds: 1500, idleBlackoutMilliseconds: 30000,
  blackoutRecoveryMilliseconds: 500, brightnessRecoveryMilliseconds: 500,
  brightnessRecoveryFrames: 5, brightnessFadeDurationMilliseconds: 300,
  brightnessFadeEnabled: true, blackoutFadeEnabled: true,
  brightnessFadeFrames: 30, brightnessFadeExponent: 2.4,
};
const presets = { Daily: daily, Movie: { ...daily, blackoutFadeEnabled: false } };
const modeNames: Record<Mode, string> = { Daily: '日常模式', Movie: '观影模式', Custom: '自定义', Off: '关闭自动暗屏' };
let state: AppState;
let draft: Settings;
let profile: Profile;
let startup = false;
let startupDirty = false;
const draftDirtyKeys = new Set<keyof Settings>();
const pendingNumericEdits = new Set<NumericProfileKey>();
let dirty = false;
let busy = false;
let stateEpoch = 0;
let pollPending = false;
let recoveryDialogTarget: string | null = null;
let recoveryDialogModel: string | null = null;
let monitorChoiceSignature = '';
let customDraft: Profile | null = null;
const systemTheme = matchMedia('(prefers-color-scheme: dark)');
const preview = !isTauri();
const appIcon = new URL('../assets/fishcreen.svg', import.meta.url).href;

const icons = {
  screen: '<rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/>',
  dim: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/>',
  return: '<path d="M9 10H3V4m0 6a9 9 0 1 1 1 8"/>',
  moon: '<path d="M20.5 13A9 9 0 0 1 11 3.5 9 9 0 1 0 20.5 13Z"/>',
  settings: '<path d="M4 7h16M4 17h16"/><circle cx="8" cy="7" r="3"/><circle cx="16" cy="17" r="3"/>',
};
function icon(name: keyof typeof icons) { return `<svg viewBox="0 0 24 24" aria-hidden="true">${icons[name]}</svg>`; }
function el<T extends HTMLElement>(id: string): T { return document.getElementById(id) as T; }
function slider(key: NumericProfileKey) {
  const s = sliders.find(v => v.key === key)!;
  return `<div class="setting-row slider-row" data-row="${s.key}">
    <div class="setting-copy"><label for="${s.key}">${s.label}</label><p id="${s.key}-hint">${s.description}</p></div>
    <div class="slider-control"><input type="range" id="${s.key}" min="${s.min}" max="${s.max}" step="${s.step}" aria-describedby="${s.key}-hint" />
      <label class="number-field"><span class="sr-only">${s.label}数值</span><input type="number" id="${s.key}-number" min="${s.min}" max="${s.max}" step="${s.step}" aria-label="${s.label}数值"/><span>${s.unit}</span></label>
    </div></div>`;
}
function toggle(id: string, title: string, description = '') {
  return `<div class="setting-row"><div class="setting-copy"><label for="${id}">${title}</label>${description ? `<p id="${id}-hint">${description}</p>` : ''}</div><label class="switch"><input type="checkbox" id="${id}"${description ? ` aria-describedby="${id}-hint"` : ''}/><span aria-hidden="true"></span></label></div>`;
}
function section(title: string, iconName: keyof typeof icons, content: string, note = '') {
  return `<section class="setting-section"><h2>${icon(iconName)}${title}</h2><div class="section-panel">${content}</div>${note ? `<p class="section-note">${note}</p>` : ''}</section>`;
}

el('app').innerHTML = `
  <header class="app-header"><div class="brand"><img class="app-icon" src="${appIcon}" alt=""/><div class="brand-copy"><h1>fishcreen<span class="version" id="version"></span></h1><div class="status-strip" role="status"><span class="status-dot" aria-hidden="true"></span><span id="live-status">正在连接调光核心…</span></div></div></div><button class="text-button" id="restore">${icon('return')}立即恢复</button></header>
  <main id="settings-content" aria-label="fishcreen 设置">
    ${preview ? '<div class="preview-note">界面预览 · 示例数据，不控制显示器</div>' : ''}
    ${section('屏幕与模式', 'screen', `
      <div class="setting-row"><div class="setting-copy"><label for="target">生效屏幕</label><p id="monitor-detail">正在读取显示器…</p></div><select id="target" aria-describedby="monitor-detail"></select></div>
      <div class="setting-row" id="recovery-binding" hidden><div class="setting-copy"><p id="recovery-binding-hint"></p></div><button class="secondary" id="confirm-recovery">确认原屏幕并恢复亮度</button></div>
      <div class="setting-row"><div class="setting-copy"><label for="mode">运行模式</label><p id="mode-hint">预设之外的修改，都会保存在自定义中</p></div><select id="mode">${Object.entries(modeNames).map(([k,v])=>`<option value="${k}">${v}</option>`).join('')}</select></div>
    `)}
    ${section('离开时，渐渐变暗', 'dim', slider('leaveDelayMilliseconds') + toggle('brightnessFadeEnabled', '亮度渐变', '使用前快后慢的曲线降低硬件亮度') + slider('brightnessFadeDurationMilliseconds') + slider('brightnessFadeFrames') + slider('brightnessFadeExponent'), '硬件亮度通过 DDC 控制。帧数较高时，显示器响应可能延长实际过渡时间。')}
    ${section('回来时，恢复亮度', 'return', slider('brightnessRecoveryMilliseconds') + slider('brightnessRecoveryFrames'))}
    ${section('空闲时，收起画面', 'moon', slider('idleBlackoutMilliseconds') + toggle('showActiveWindow', '显示活跃窗口', '关闭时完全覆盖；开启时不遮挡当前前台窗口') + toggle('blackoutFadeEnabled', '黑幕渐变', '日常与自定义模式下，用 5 秒渐变进入黑屏') + slider('blackoutRecoveryMilliseconds'))}
    ${section('常规', 'settings', `
      <div class="setting-row"><div class="setting-copy"><label for="theme">界面外观</label></div><select id="theme"><option value="System">跟随系统</option><option value="Light">明亮</option><option value="Dark">黑暗</option></select></div>
      ${toggle('startup', '开机自动启动')}
    `)}
    <p class="page-footnote">关闭设置窗口后，fishcreen 会继续在托盘运行。</p>
  </main>
  <footer class="action-bar"><p id="save-status" role="status" aria-live="polite">设置已保存</p><div><button id="cancel" class="secondary">取消</button><button id="apply" class="secondary" disabled>应用</button><button id="save" class="primary">保存</button></div></footer>
  <div class="error-banner" id="error" role="alert" hidden></div>
  <dialog id="recovery-dialog" aria-labelledby="recovery-dialog-title" aria-describedby="recovery-dialog-description"><h2 id="recovery-dialog-title">确认原来的物理屏幕</h2><p id="recovery-dialog-description"></p><form method="dialog"><button value="cancel" class="secondary">取消</button><button value="confirm" class="primary">确认并恢复</button></form></dialog>`;

function setTheme(value: ThemeMode) {
  document.documentElement.dataset.theme = value === 'System' ? (systemTheme.matches ? 'dark' : 'light') : value.toLowerCase();
}
function setDirty(...keys: (keyof Settings)[]) {
  for (const key of keys) draftDirtyKeys.add(key);
  dirty = true;
  el('save-status').textContent = '有未应用的修改';
  el<HTMLButtonElement>('apply').disabled = busy;
}
function customMode() {
  if (draft.mode !== 'Custom') { draft.mode = 'Custom'; el<HTMLSelectElement>('mode').value = 'Custom'; }
  draft.customProfile = structuredClone(profile);
  customDraft = structuredClone(profile);
  updateModeHint();
  setDirty('mode', 'customProfile');
}
function updateModeHint() {
  el('mode-hint').textContent = draft.mode === 'Custom' ? '当前参数保存在自定义中，不影响固定预设' : draft.mode === 'Off' ? '自动调暗与黑幕已关闭' : '固定预设 · 调整任意参数即切换到自定义';
}
function updateControls() {
  for (const s of sliders) {
    const v = Number(profile[s.key]) / (s.scale ?? 1);
    el<HTMLInputElement>(s.key).value = String(v);
    if (!pendingNumericEdits.has(s.key)) {
      el<HTMLInputElement>(`${s.key}-number`).value = s.step < 1 && Math.abs(v * 10 - Math.round(v * 10)) < 1e-8 ? v.toFixed(1) : String(v);
    }
    el<HTMLInputElement>(s.key).style.setProperty('--fill', `${(v-s.min)/(s.max-s.min)*100}%`);
  }
  el<HTMLInputElement>('brightnessFadeEnabled').checked = profile.brightnessFadeEnabled;
  el<HTMLInputElement>('blackoutFadeEnabled').checked = profile.blackoutFadeEnabled;
  for (const key of ['brightnessFadeDurationMilliseconds','brightnessFadeFrames','brightnessFadeExponent'] as const) {
    const enabled = profile.brightnessFadeEnabled && (key === 'brightnessFadeDurationMilliseconds' || profile.brightnessFadeDurationMilliseconds > 0);
    el<HTMLInputElement>(key).disabled = !enabled;
    el<HTMLInputElement>(`${key}-number`).disabled = !enabled;
    document.querySelector(`[data-row="${key}"]`)!.classList.toggle('is-disabled', !enabled);
  }
  const enabled = profile.brightnessRecoveryMilliseconds > 0;
  el<HTMLInputElement>('brightnessRecoveryFrames').disabled = !enabled;
  el<HTMLInputElement>('brightnessRecoveryFrames-number').disabled = !enabled;
  document.querySelector('[data-row="brightnessRecoveryFrames"]')!.classList.toggle('is-disabled', !enabled);
}
function updateMonitorDetail() {
  const selected = state.monitors.find(m => m.stableId === draft.targetStableId);
  el('monitor-detail').textContent = selected ? `${selected.bounds.right-selected.bounds.left} × ${selected.bounds.bottom-selected.bounds.top}${selected.primary ? ' · 主屏' : ' · 副屏'}` : '当前未连接或旧版选择需重新绑定 · 自动控制将暂停';
  const pending = state.recoveryRequiresConfirmation;
  el('recovery-binding').hidden = !pending;
  el('recovery-binding-hint').textContent = pending ? `旧版 ${pending} 亮度恢复记录需要确认原来的物理屏幕。请选择原屏幕，确认后才会恢复；不会应用其他未保存修改。` : '';
  el<HTMLButtonElement>('confirm-recovery').disabled = busy || !selected;
}
function updateMonitorChoices() {
  const signature = JSON.stringify([draft.targetStableId, state.monitors.map(m => [m.stableId, m.name, m.primary])]);
  if (signature !== monitorChoiceSignature) {
    const choices = state.monitors.map(m => new Option(`${m.name}${m.primary ? '（主屏）' : ''}`, m.stableId));
    if (!state.monitors.some(m => m.stableId === draft.targetStableId)) choices.push(new Option('已选择的屏幕（未连接或需重新绑定）', draft.targetStableId));
    const target = el<HTMLSelectElement>('target');
    target.replaceChildren(...choices);
    target.value = draft.targetStableId;
    monitorChoiceSignature = signature;
  }
  updateMonitorDetail();
}
function renderState(next: AppState, reset: boolean) {
  state = next;
  el('version').textContent = next.version;
  el('live-status').textContent = next.status;
  if (reset) {
    draft = structuredClone(next.settings);
    draftDirtyKeys.clear(); startupDirty = false; dirty = false;
    pendingNumericEdits.clear();
    for (const spec of sliders) clearNumberError(el<HTMLInputElement>(`${spec.key}-number`));
  } else {
    draft = mergeSettingsDraft(draft, next.settings, draftDirtyKeys);
  }
  if (reset || (!draftDirtyKeys.has('mode') && !draftDirtyKeys.has('customProfile'))) {
    profile = structuredClone(draft.mode === 'Daily' || draft.mode === 'Movie' ? presets[draft.mode] : draft.customProfile);
  }
  if (reset || !draftDirtyKeys.has('customProfile')) customDraft = structuredClone(draft.customProfile);
  if (!startupDirty) startup = next.startupEnabled;
  updateMonitorChoices();
  el<HTMLSelectElement>('mode').value = draft.mode;
  el<HTMLSelectElement>('theme').value = draft.themeMode;
  el<HTMLInputElement>('startup').checked = startup;
  el<HTMLInputElement>('showActiveWindow').checked = draft.showActiveWindow;
  updateModeHint();
  // Update untouched controls too; updateControls preserves in-progress text.
  updateControls();
  setTheme(draft.themeMode);
  el<HTMLButtonElement>('apply').disabled = busy || !dirty;
}

function invalidNumber(source: HTMLInputElement, message: string) {
  source.setCustomValidity(message);
  source.setAttribute('aria-invalid', 'true');
}
function clearNumberError(source: HTMLInputElement) {
  source.setCustomValidity('');
  source.removeAttribute('aria-invalid');
}
function commitNumber(spec: SliderSpec, source: HTMLInputElement) {
  try {
    const value = quantizeNumericInput(spec, source.value);
    clearNumberError(source);
    pendingNumericEdits.delete(spec.key);
    if (profile[spec.key] !== value) { profile[spec.key] = value; customMode(); }
    updateControls();
  } catch (error) {
    pendingNumericEdits.add(spec.key);
    invalidNumber(source, String(error instanceof Error ? error.message : error));
    setDirty();
  }
}
function validateNumbers(): boolean {
  const values = new Map<NumericProfileKey, number>();
  let firstInvalid: HTMLInputElement | null = null;
  for (const spec of sliders) {
    const input = el<HTMLInputElement>(`${spec.key}-number`);
    if (input.disabled) continue;
    try { values.set(spec.key, quantizeNumericInput(spec, input.value)); clearNumberError(input); }
    catch (error) { invalidNumber(input, String(error instanceof Error ? error.message : error)); firstInvalid ??= input; }
  }
  if (firstInvalid) {
    showError(firstInvalid.validationMessage); firstInvalid.focus(); firstInvalid.reportValidity(); return false;
  }
  let changed = false;
  for (const [key, value] of values) {
    pendingNumericEdits.delete(key);
    if (profile[key] !== value) { profile[key] = value; changed = true; }
  }
  if (changed) customMode();
  const invalid = profileValidationError(profile) ?? profileValidationError(draft.customProfile);
  if (invalid) {
    const input = el<HTMLInputElement>(`${invalid.key}-number`);
    invalidNumber(input, invalid.message); showError(invalid.message); input.focus(); input.reportValidity(); return false;
  }
  updateControls();
  return true;
}
for (const s of sliders) {
  el<HTMLInputElement>(s.key).addEventListener('input', e => { if (draft && !busy) commitNumber(s, e.target as HTMLInputElement); });
  const number = el<HTMLInputElement>(`${s.key}-number`);
  number.addEventListener('input', () => { if (draft && !busy) { pendingNumericEdits.add(s.key); clearNumberError(number); setDirty(); } });
  number.addEventListener('change', () => { if (draft && !busy) commitNumber(s, number); });
}
for (const key of ['brightnessFadeEnabled','blackoutFadeEnabled'] as const) {
  el<HTMLInputElement>(key).addEventListener('change', e => { profile[key] = (e.target as HTMLInputElement).checked; customMode(); updateControls(); });
}
el<HTMLSelectElement>('target').addEventListener('change', e => { draft.targetStableId = (e.target as HTMLSelectElement).value; updateMonitorDetail(); setDirty('targetStableId'); });
el<HTMLSelectElement>('mode').addEventListener('change', e => {
  if (draft.mode === 'Custom') customDraft = structuredClone(profile);
  draft.mode = (e.target as HTMLSelectElement).value as Mode;
  profile = structuredClone(draft.mode === 'Daily' || draft.mode === 'Movie' ? presets[draft.mode] : customDraft ?? draft.customProfile);
  if (draft.mode === 'Custom') draft.customProfile = structuredClone(profile);
  pendingNumericEdits.clear();
  for (const spec of sliders) clearNumberError(el<HTMLInputElement>(`${spec.key}-number`));
  updateModeHint(); updateControls(); setDirty('mode');
});
el<HTMLSelectElement>('theme').addEventListener('change', e => { draft.themeMode = (e.target as HTMLSelectElement).value as ThemeMode; setTheme(draft.themeMode); setDirty('themeMode'); });
systemTheme.addEventListener('change', () => { if (draft?.themeMode === 'System') setTheme('System'); });
el<HTMLInputElement>('startup').addEventListener('change', e => { startup = (e.target as HTMLInputElement).checked; startupDirty = true; setDirty(); });
el<HTMLInputElement>('showActiveWindow').addEventListener('change', e => { draft.showActiveWindow = (e.target as HTMLInputElement).checked; setDirty('showActiveWindow'); });

function showError(error: unknown) {
  el('error').textContent = `操作未完成：${String(error)}`;
  el('error').hidden = false;
  el('save-status').textContent = '修改尚未保存';
}
function setBusy(value: boolean) {
  busy = value;
  el('settings-content').inert = value;
  el<HTMLButtonElement>('restore').disabled = value;
  el<HTMLButtonElement>('cancel').disabled = value;
  el<HTMLButtonElement>('save').disabled = value;
  el<HTMLButtonElement>('apply').disabled = value || !dirty;
  updateMonitorDetail();
}
async function close() { if (!preview) await invoke('close_settings'); }
async function apply(closeAfter: boolean) {
  if (busy || !draft) return;
  if (!validateNumbers()) return;
  ++stateEpoch;
  setBusy(true); el('error').hidden = true;
  try {
    if (draft.mode === 'Custom') draft.customProfile = structuredClone(profile);
    const next = preview ? { ...state, settings: structuredClone(draft), startupEnabled: startupDirty ? startup : state.startupEnabled } : await invoke<AppState>('apply_settings', { settings: draft, startupEnabled: startupDirty ? startup : null });
    renderState(next, true);
    el('save-status').textContent = closeAfter ? '设置已保存' : '设置已应用';
    if (closeAfter) await close();
  } catch (error) { showError(error); } finally { setBusy(false); }
}
el('apply').addEventListener('click', () => void apply(false));
el('save').addEventListener('click', () => void apply(true));
el('cancel').addEventListener('click', () => void close());
el('restore').addEventListener('click', async () => {
  if (busy) return;
  ++stateEpoch;
  setBusy(true);
  try { if (!preview) await invoke('restore_brightness'); el('save-status').textContent = '已请求恢复亮度与黑幕'; }
  catch (error) { showError(error); } finally { setBusy(false); }
});
el('confirm-recovery').addEventListener('click', () => {
  const selected = state.monitors.find(m => m.stableId === draft.targetStableId);
  if (busy || !selected || !state.recoveryRequiresConfirmation) return;
  recoveryDialogTarget = selected.stableId;
  recoveryDialogModel = state.recoveryRequiresConfirmation;
  el('recovery-dialog-description').textContent = `请确认“${selected.name}”就是之前被调暗的原物理屏幕，而不是同型号的新屏幕。确认后会恢复旧记录中的亮度，其他未保存设置不会应用。`;
  const dialog = el<HTMLDialogElement>('recovery-dialog');
  dialog.returnValue = 'cancel';
  dialog.showModal();
});
el<HTMLDialogElement>('recovery-dialog').addEventListener('close', async () => {
  const dialog = el<HTMLDialogElement>('recovery-dialog');
  const target = recoveryDialogTarget;
  const model = recoveryDialogModel;
  recoveryDialogTarget = null; recoveryDialogModel = null;
  if (dialog.returnValue !== 'confirm' || busy || !target) return;
  if (!state.monitors.some(m => m.stableId === target) || !model || state.recoveryRequiresConfirmation !== model) {
    showError('屏幕连接或恢复记录已变化，请重新确认'); return;
  }
  ++stateEpoch; setBusy(true); el('error').hidden = true;
  try {
    const next = preview ? { ...state, recoveryRequiresConfirmation: null } : await invoke<AppState>('confirm_recovery_target', { target });
    renderState(next, false);
    el('save-status').textContent = dirty ? '亮度已恢复，其他修改尚未应用' : '已确认并恢复原屏幕亮度';
  } catch (error) {
    if (!preview) {
      try { renderState(await invoke<AppState>('get_state'), false); } catch { /* Keep the original recovery error visible. */ }
    }
    showError(error);
  } finally { setBusy(false); }
});
document.addEventListener('keydown', e => { if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); void apply(false); } });

async function init() {
  setTheme('Dark');
  try {
    const initial: AppState = preview ? {
      settings: { targetStableId: String.raw`\\?\DISPLAY#DELA0E7#PREVIEW#{GUID}`, mode: 'Daily', themeMode: 'Dark', showActiveWindow: false, customProfile: structuredClone(daily) },
      monitors: [{ stableId: String.raw`\\?\DISPLAY#DELA0E7#PREVIEW#{GUID}`, name: 'Dell S2417DG', gdiName: '\\\\.\\DISPLAY1', bounds: { left: -1440, top: 0, right: 0, bottom: 2560 }, primary: false }],
      status: '日常模式 · 等待鼠标活动', brightnessDimmed: false, overlayOpacity: 0, startupEnabled: false, version: '1.2.1', shell: 'Tauri',
    } : await invoke<AppState>('get_state');
    renderState(initial, true);
    if (!preview) schedulePoll();
  } catch (error) { showError(error); }
}
function schedulePoll() { setTimeout(() => void pollState(), 2000); }
async function pollState() {
  if (pollPending) return;
  if (document.hidden || busy) { schedulePoll(); return; }
  pollPending = true;
  const epoch = stateEpoch;
  try {
    const next = await invoke<AppState>('get_state');
    if (!busy && epoch === stateEpoch) renderState(next, false);
  } catch { /* A transient status failure must not discard the editable draft. */ }
  finally { pollPending = false; schedulePoll(); }
}
void init();
