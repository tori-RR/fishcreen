import { invoke, isTauri } from '@tauri-apps/api/core';
import type { AppState, Mode, Profile, Settings, ThemeMode } from './types';
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
let dirty = false;
let busy = false;
let customDraft: Profile | null = null;
const systemTheme = matchMedia('(prefers-color-scheme: dark)');
const preview = !isTauri();
const appIcon = new URL('../assets/fishcreen.png', import.meta.url).href;

const icons = {
  screen: '<rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/>',
  dim: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5"/>',
  return: '<path d="M9 10H3V4m0 6a9 9 0 1 1 1 8"/>',
  moon: '<path d="M20.5 13A9 9 0 0 1 11 3.5 9 9 0 1 0 20.5 13Z"/>',
  settings: '<path d="M4 7h16M4 17h16"/><circle cx="8" cy="7" r="3"/><circle cx="16" cy="17" r="3"/>',
};
function icon(name: keyof typeof icons) { return `<svg viewBox="0 0 24 24" aria-hidden="true">${icons[name]}</svg>`; }
function el<T extends HTMLElement>(id: string): T { return document.getElementById(id) as T; }
function escapeText(value: string): string { const n = document.createElement('span'); n.textContent = value; return n.innerHTML; }

interface SliderSpec { key: keyof Profile; label: string; description: string; min: number; max: number; step: number; scale?: number; unit: string; }
const sliders: SliderSpec[] = [
  { key: 'leaveDelayMilliseconds', label: '鼠标离开后变暗', description: '离开所选屏幕后，等待多久开始调暗', min: .5, max: 10, step: .1, scale: 1000, unit: '秒' },
  { key: 'brightnessFadeDurationMilliseconds', label: '亮度变暗时间', description: '0 秒为立即变暗', min: 0, max: 3, step: .1, scale: 1000, unit: '秒' },
  { key: 'brightnessFadeFrames', label: '亮度变暗帧数', description: '帧数越高，亮度变化越细腻', min: 5, max: 60, step: 1, unit: '帧' },
  { key: 'brightnessFadeExponent', label: '曲线指数', description: '数值越高，前段变暗越快、后段越慢', min: 1, max: 4, step: .1, unit: '' },
  { key: 'brightnessRecoveryMilliseconds', label: '亮度恢复时间', description: '鼠标进入所选屏幕时恢复；0 秒为立即恢复', min: 0, max: 1, step: .1, scale: 1000, unit: '秒' },
  { key: 'brightnessRecoveryFrames', label: '亮度恢复帧数', description: '恢复硬件亮度时的过渡帧数', min: 1, max: 30, step: 1, unit: '帧' },
  { key: 'idleBlackoutMilliseconds', label: '鼠标无动作后黑屏', description: '全局鼠标静止达到此时间后，开始显示黑幕', min: 5, max: 300, step: 1, scale: 1000, unit: '秒' },
  { key: 'blackoutRecoveryMilliseconds', label: '黑幕恢复时间', description: '鼠标活动后，黑幕恢复到完全透明的时间', min: .1, max: 5, step: .1, scale: 1000, unit: '秒' },
];
function slider(key: keyof Profile) {
  const s = sliders.find(v => v.key === key)!;
  return `<div class="setting-row slider-row" data-row="${s.key}">
    <div class="setting-copy"><label for="${s.key}">${s.label}</label><p id="${s.key}-hint">${s.description}</p></div>
    <div class="slider-control"><input type="range" id="${s.key}" min="${s.min}" max="${s.max}" step="${s.step}" aria-describedby="${s.key}-hint" />
      <label class="number-field"><span class="sr-only">${s.label}数值</span><input type="number" id="${s.key}-number" min="${s.min}" max="${s.max}" step="${s.step}" aria-label="${s.label}数值"/><span>${s.unit}</span></label>
    </div></div>`;
}
function toggle(id: string, title: string, description: string) {
  return `<div class="setting-row"><div class="setting-copy"><label for="${id}">${title}</label><p id="${id}-hint">${description}</p></div><label class="switch"><input type="checkbox" id="${id}" aria-describedby="${id}-hint"/><span aria-hidden="true"></span></label></div>`;
}
function section(title: string, iconName: keyof typeof icons, content: string, note = '') {
  return `<section class="setting-section"><h2>${icon(iconName)}${title}</h2><div class="section-panel">${content}</div>${note ? `<p class="section-note">${note}</p>` : ''}</section>`;
}

el('app').innerHTML = `
  <header class="app-header"><div class="brand"><img class="app-icon" src="${appIcon}" alt=""/><div><h1>fishcreen<span class="version" id="version">Rust + Tauri</span></h1><p>让副屏安静一点。</p></div></div><button class="text-button" id="restore">${icon('return')}立即恢复</button></header>
  <main id="settings-content" aria-label="fishcreen 设置">
    ${preview ? '<div class="preview-note">界面预览 · 示例数据，不控制显示器</div>' : ''}
    ${section('屏幕与模式', 'screen', `
      <div class="setting-row"><div class="setting-copy"><label for="target">生效屏幕</label><p id="monitor-detail">正在读取显示器…</p></div><select id="target" aria-describedby="monitor-detail"></select></div>
      <div class="setting-row"><div class="setting-copy"><label for="mode">运行模式</label><p id="mode-hint">预设之外的修改，都会保存在自定义中</p></div><select id="mode">${Object.entries(modeNames).map(([k,v])=>`<option value="${k}">${v}</option>`).join('')}</select></div>
    `)}
    <div class="status-strip" role="status"><span class="status-dot"></span><span id="live-status">正在连接调光核心…</span></div>
    ${section('离开时，渐渐变暗', 'dim', slider('leaveDelayMilliseconds') + toggle('brightnessFadeEnabled', '亮度渐变', '使用前快后慢的曲线降低硬件亮度') + slider('brightnessFadeDurationMilliseconds') + slider('brightnessFadeFrames') + slider('brightnessFadeExponent'), '硬件亮度通过 DDC 控制。帧数较高时，显示器响应可能延长实际过渡时间。')}
    ${section('回来时，恢复亮度', 'return', slider('brightnessRecoveryMilliseconds') + slider('brightnessRecoveryFrames'))}
    ${section('空闲时，收起画面', 'moon', slider('idleBlackoutMilliseconds') + toggle('blackoutFadeEnabled', '黑幕渐变', '日常与自定义模式下，用 5 秒渐变进入黑屏') + slider('blackoutRecoveryMilliseconds'))}
    ${section('常规', 'settings', `
      <div class="setting-row"><div class="setting-copy"><label for="theme">界面外观</label><p>根据使用环境，选择舒服的明暗</p></div><select id="theme"><option value="System">跟随系统</option><option value="Light">明亮</option><option value="Dark">黑暗</option></select></div>
      ${toggle('startup', '开机自动启动', '登录 Windows 后，在托盘中运行')}
    `)}
    <p class="page-footnote">关闭设置窗口后，fishcreen 会继续在托盘运行。</p>
  </main>
  <footer class="action-bar"><p id="save-status" role="status" aria-live="polite">设置已保存</p><div><button id="cancel" class="secondary">取消</button><button id="apply" class="secondary" disabled>应用</button><button id="save" class="primary">保存</button></div></footer>
  <div class="error-banner" id="error" role="alert" hidden></div>`;

function setTheme(value: ThemeMode) {
  document.documentElement.dataset.theme = value === 'System' ? (systemTheme.matches ? 'dark' : 'light') : value.toLowerCase();
}
function setDirty() {
  dirty = true;
  el('save-status').textContent = '有未应用的修改';
  el<HTMLButtonElement>('apply').disabled = busy;
}
function customMode() {
  if (draft.mode !== 'Custom') { draft.mode = 'Custom'; el<HTMLSelectElement>('mode').value = 'Custom'; }
  draft.customProfile = structuredClone(profile);
  customDraft = structuredClone(profile);
  updateModeHint();
  setDirty();
}
function updateModeHint() {
  el('mode-hint').textContent = draft.mode === 'Custom' ? '当前参数保存在自定义中，不影响固定预设' : draft.mode === 'Off' ? '自动调暗与黑幕已关闭' : '固定预设 · 调整任意参数即切换到自定义';
}
function updateControls() {
  for (const s of sliders) {
    const v = Number(profile[s.key]) / (s.scale ?? 1);
    el<HTMLInputElement>(s.key).value = String(v);
    el<HTMLInputElement>(`${s.key}-number`).value = s.step < 1 ? v.toFixed(1) : String(v);
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
  el('monitor-detail').textContent = selected ? `${selected.bounds.right-selected.bounds.left} × ${selected.bounds.bottom-selected.bounds.top}${selected.primary ? ' · 主屏' : ' · 副屏'}` : '当前未连接 · 自动控制将暂停';
}
function renderState(next: AppState, reset: boolean) {
  state = next;
  el('version').textContent = `${next.version} · ${next.shell}`;
  el('live-status').textContent = next.status;
  if (!reset) return;
  draft = structuredClone(next.settings);
  profile = structuredClone(draft.mode === 'Daily' || draft.mode === 'Movie' ? presets[draft.mode] : draft.customProfile);
  customDraft = structuredClone(draft.customProfile);
  startup = next.startupEnabled;
  const choices = next.monitors.map(m => `<option value="${escapeText(m.stableId)}">${escapeText(m.name)}${m.primary ? '（主屏）' : ''}</option>`);
  if (!next.monitors.some(m => m.stableId === draft.targetStableId)) choices.push(`<option value="${escapeText(draft.targetStableId)}">已选择的屏幕（未连接）</option>`);
  el<HTMLSelectElement>('target').innerHTML = choices.join('');
  el<HTMLSelectElement>('target').value = draft.targetStableId;
  el<HTMLSelectElement>('mode').value = draft.mode;
  el<HTMLSelectElement>('theme').value = draft.themeMode;
  el<HTMLInputElement>('startup').checked = startup;
  updateMonitorDetail(); updateModeHint(); updateControls(); setTheme(draft.themeMode);
  dirty = false;
  el<HTMLButtonElement>('apply').disabled = true;
}

for (const s of sliders) {
  const edit = (source: HTMLInputElement, commit: boolean) => {
    if (!draft || busy) return;
    if (source.value === '' && !commit) return;
    const raw = Number(source.value);
    if (!Number.isFinite(raw)) return;
    const value = Math.max(s.min, Math.min(s.max, raw));
    (profile[s.key] as number) = Number((value * (s.scale ?? 1)).toFixed(3));
    customMode(); updateControls();
  };
  el<HTMLInputElement>(s.key).addEventListener('input', e => edit(e.target as HTMLInputElement, true));
  el<HTMLInputElement>(`${s.key}-number`).addEventListener('change', e => edit(e.target as HTMLInputElement, true));
}
for (const key of ['brightnessFadeEnabled','blackoutFadeEnabled'] as const) {
  el<HTMLInputElement>(key).addEventListener('change', e => { profile[key] = (e.target as HTMLInputElement).checked; customMode(); updateControls(); });
}
el<HTMLSelectElement>('target').addEventListener('change', e => { draft.targetStableId = (e.target as HTMLSelectElement).value; updateMonitorDetail(); setDirty(); });
el<HTMLSelectElement>('mode').addEventListener('change', e => {
  if (draft.mode === 'Custom') customDraft = structuredClone(profile);
  draft.mode = (e.target as HTMLSelectElement).value as Mode;
  profile = structuredClone(draft.mode === 'Daily' || draft.mode === 'Movie' ? presets[draft.mode] : customDraft ?? draft.customProfile);
  if (draft.mode === 'Custom') draft.customProfile = structuredClone(profile);
  updateModeHint(); updateControls(); setDirty();
});
el<HTMLSelectElement>('theme').addEventListener('change', e => { draft.themeMode = (e.target as HTMLSelectElement).value as ThemeMode; setTheme(draft.themeMode); setDirty(); });
systemTheme.addEventListener('change', () => { if (draft?.themeMode === 'System') setTheme('System'); });
el<HTMLInputElement>('startup').addEventListener('change', e => { startup = (e.target as HTMLInputElement).checked; setDirty(); });

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
}
async function close() { if (!preview) await invoke('close_settings'); }
async function apply(closeAfter: boolean) {
  if (busy || !draft) return;
  setBusy(true); el('error').hidden = true;
  try {
    if (draft.mode === 'Custom') draft.customProfile = structuredClone(profile);
    const next = preview ? { ...state, settings: structuredClone(draft), startupEnabled: startup } : await invoke<AppState>('apply_settings', { settings: draft, startupEnabled: startup });
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
  setBusy(true);
  try { if (!preview) await invoke('restore_brightness'); el('save-status').textContent = '已请求恢复亮度与黑幕'; }
  catch (error) { showError(error); } finally { setBusy(false); }
});
document.addEventListener('keydown', e => { if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); void apply(false); } });

async function init() {
  setTheme('Dark');
  try {
    const initial: AppState = preview ? {
      settings: { targetStableId: 'DELA0E7', mode: 'Daily', themeMode: 'Dark', customProfile: structuredClone(daily) },
      monitors: [{ stableId: 'DELA0E7', name: 'Dell S2417DG', gdiName: '\\\\.\\DISPLAY1', bounds: { left: -1440, top: 0, right: 0, bottom: 2560 }, primary: false }],
      status: '日常模式 · 等待鼠标活动', brightnessDimmed: false, overlayOpacity: 0, startupEnabled: false, version: '1.2.0', shell: 'Tauri',
    } : await invoke<AppState>('get_state');
    renderState(initial, true);
    if (!preview) setInterval(async () => {
      if (document.hidden || busy) return;
      try {
        const next = await invoke<AppState>('get_state');
        const changedElsewhere = !dirty && JSON.stringify(next.settings) !== JSON.stringify(state.settings);
        renderState(next, changedElsewhere);
      } catch { /* Transient status polling does not discard edits. */ }
    }, 2000);
  } catch (error) { showError(error); }
}
void init();
